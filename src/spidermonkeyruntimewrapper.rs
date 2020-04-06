use crate::es_utils;
use crate::es_utils::EsErrorInfo;
use crate::esruntimewrapper::EsRuntimeWrapper;

use crate::esvaluefacade::EsValueFacade;
use log::{debug, trace};

use crate::es_utils::rooting::EsPersistentRooted;
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use lru::LruCache;
use mozjs::jsapi::CallArgs;
use mozjs::jsapi::CompartmentOptions;
use mozjs::jsapi::JSAutoCompartment;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFunction;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::JS_NewGlobalObject;
use mozjs::jsapi::JS_ReportErrorASCII;
use mozjs::jsapi::OnNewGlobalHookOption;
use mozjs::jsapi::SetEnqueuePromiseJobCallback;
use mozjs::jsapi::SetModuleResolveHook;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::panic::wrap_panic;
use mozjs::rust::wrappers::JS_CallFunctionValue;
use mozjs::rust::{HandleObject, HandleValue, JSEngine};
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::panic::AssertUnwindSafe;
use std::ptr;
use std::rc::Rc;
use std::str;
use std::sync::{Arc, Weak};

/// the type for registering rust_ops in the script engine
pub type OP = Box<dyn Fn(&SmRuntime, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send>;

/// wrapper for the SpiderMonkey runtime
pub struct SmRuntime {
    runtime: mozjs::rust::Runtime,
    global_obj: *mut JSObject,
    op_container: HashMap<String, OP>,
    pub(crate) opt_es_rt_inner: Option<Weak<EsRuntimeWrapperInner>>,
}

thread_local! {
    /// the thread-local SpiderMonkeyRuntime
    /// this only exists for the worker thread of the MicroTaskManager
    pub(crate) static SM_RT: RefCell<SmRuntime> = RefCell::new(SmRuntime::new());
}

lazy_static! {
/// the reusable ENGINE object which is required to construct a new Runtime
    static ref ENGINE: Arc<JSEngine> = { JSEngine::init().unwrap() };
}

impl SmRuntime {
    pub fn clone_rtw_inner(&self) -> Arc<EsRuntimeWrapperInner> {
        self.opt_es_rt_inner
            .as_ref()
            .expect("not initted yet")
            .upgrade()
            .expect("parent EsRuntimeWrapperInner was dropped")
    }
    /// construct a new SmRuntime, this should only be called from the workerthread of the MicroTaskManager
    /// here we actualy construct a new Runtime
    fn new() -> Self {
        debug!("init SmRuntime {}", thread_id::get());
        let engine: Arc<JSEngine> = ENGINE.clone();

        let runtime = mozjs::rust::Runtime::new(engine);
        let context = runtime.cx();
        let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
        let c_option = CompartmentOptions::default();

        let global_obj;

        unsafe {
            global_obj = JS_NewGlobalObject(
                context,
                &SIMPLE_GLOBAL_CLASS,
                ptr::null_mut(),
                h_option,
                &c_option,
            );
        }

        let ret = SmRuntime {
            runtime,
            global_obj,
            op_container: HashMap::new(),
            opt_es_rt_inner: None,
        };

        ret.init_native_ops();
        ret.init_promise_callbacks();
        ret.init_import_callbacks();

        ret
    }

    fn init_native_ops(&self) {
        self.do_with_jsapi(|_rt, cx, global| unsafe {
            let function = JS_DefineFunction(
                cx,
                global.into(),
                b"__log\0".as_ptr() as *const libc::c_char,
                Some(log),
                1,
                0,
            );
            assert!(!function.is_null());
            let function = JS_DefineFunction(
                cx,
                global.into(),
                b"__invoke_rust_op\0".as_ptr() as *const libc::c_char,
                Some(invoke_rust_op),
                1,
                0,
            );
            assert!(!function.is_null());
        });
    }

    fn init_promise_callbacks(&self) {
        // this tells JSAPI how to schedule jobs for Promises
        self.do_with_jsapi(|_rt, cx, _global| unsafe {
            SetEnqueuePromiseJobCallback(cx, Some(enqueue_job), ptr::null_mut());
        });
    }

    fn init_import_callbacks(&self) {
        // this tells the runtime how to resolve modules
        self.do_with_jsapi(|_rt, cx, _global| {
            let func: *mut JSFunction =
                es_utils::functions::new_native_function(cx, "import_module", Some(import_module));
            rooted!(in (cx) let func_root = func);

            unsafe {
                SetModuleResolveHook(cx, func_root.handle().into());
            }
        });
    }

    /// call a function by name, the function needs to be defined on the root of the global scope
    pub fn call(
        &self,
        obj_names: Vec<&str>,
        func_name: &str,
        arguments: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_jsapi(|rt, cx, global| {
            trace!("smrt.call {} in thread {}", func_name, thread_id::get());

            self.call_obj_method_name(rt, cx, global, global, obj_names, func_name, arguments)
        })
    }

    pub fn load_module(&self, module_src: &str, module_file_name: &str) -> Result<(), EsErrorInfo> {
        trace!(
            "smrt.load_module {} in thread {}",
            module_file_name,
            thread_id::get()
        );

        self.do_with_jsapi(|_rt, cx, _global| {
            // todo cache modules like promise callbacks
            //

            let load_res = es_utils::modules::compile_module(cx, module_src, module_file_name);

            if let Some(err) = load_res.err() {
                return Err(err);
            }

            return Ok(());
        })
    }

    /// eval a piece of script and return the result as a EsValueFacade
    pub fn eval(&self, eval_code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("smrt.eval {} in thread {}", file_name, thread_id::get());

        self.do_with_jsapi(|rt, cx, global| {
            rooted!(in (cx) let mut rval = UndefinedValue());
            let eval_res: Result<(), EsErrorInfo> =
                es_utils::eval(rt, global, eval_code, file_name, rval.handle_mut());

            if eval_res.is_ok() {
                return Ok(EsValueFacade::new_v(rt, cx, global, rval.handle()));
            } else {
                return Err(eval_res.err().unwrap());
            }
        })
    }

    /// eval a piece of script and ignore the result
    pub fn eval_void(&self, eval_code: &str, file_name: &str) -> Result<(), EsErrorInfo> {
        trace!(
            "smrt.eval_void {} in thread {}",
            eval_code,
            thread_id::get()
        );

        self.do_with_jsapi(|rt, cx, global| {
            rooted!(in (cx) let mut rval = UndefinedValue());
            let eval_res: Result<(), EsErrorInfo> =
                es_utils::eval(rt, global, eval_code, file_name, rval.handle_mut());

            if eval_res.is_ok() {
                return Ok(());
            } else {
                return Err(eval_res.err().unwrap());
            }
        })
    }

    /// run the cleanup function and run the garbage collector
    /// this also fires a pre-cleanup event in script so scripts can do a cleanup before the garbage collector runs
    pub fn cleanup(&self) {
        self.do_with_jsapi(|_rt, cx, global| {
            trace!("running gc cleanup / 1");
            {
                rooted!(in (cx) let mut ret_val = UndefinedValue());
                let cleanup_res = es_utils::functions::call_method_name(
                    cx,
                    global,
                    "_esses_cleanup",
                    vec![],
                    &mut ret_val.handle_mut(),
                );
                if cleanup_res.is_err() {
                    let err = cleanup_res.err().unwrap();
                    debug!(
                        "cleanup failed: {}:{}:{} -> {}",
                        err.filename, err.lineno, err.column, err.message
                    );
                }
            }
            es_utils::gc(cx);
        });
        trace!("cleaning up sm_rt / 5");
    }
    pub fn register_op(&mut self, name: &str, op: OP) {
        let op_map = &mut self.op_container;
        op_map.insert(name.to_string(), op);
    }
    pub fn invoke_op(
        &self,
        name: String,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, String> {
        let op_map = &self.op_container;
        let op = op_map.get(&name).expect("no such op");

        let ret = op(self, args);

        return ret;
    }
    pub(crate) fn do_with_sm_rt_async<F>(&self, f: F)
    where
        F: FnOnce(&SmRuntime) + Send + 'static,
    {
        self.opt_es_rt_inner
            .as_ref()
            .unwrap()
            .upgrade()
            .expect("parent EsRuntimeWrapperInner was dropped")
            .do_in_es_runtime_thread(Box::new(move |sm_rt: &SmRuntime| {
                f(sm_rt);
            }));
    }
    #[allow(dead_code)]
    pub(crate) fn do_with_esses_rt_async<F>(&self, f: F)
    where
        F: FnOnce(&EsRuntimeWrapperInner) + Send + 'static,
    {
        // add job to esruntime.helpertasks, clone inner and pass it?
        //self.opt_es_rt_inner.unwrap().do_in_helper_thread();

        let inner_clone = self
            .opt_es_rt_inner
            .as_ref()
            .unwrap()
            .upgrade()
            .expect("parent EsRuntimeWrapperInner was dropped");
        EsRuntimeWrapper::add_helper_task(move || {
            f(&*inner_clone);
        });
    }

    /// call a method by name on an object by name
    /// e.g. esses.cleanup() can be called by calling
    /// call_obj_method_name(cx, glob, vec!["esses"], "cleanup", vec![]);
    #[allow(dead_code)]
    fn call_obj_method_name(
        &self,
        rt: &Runtime,
        context: *mut JSContext,
        global: HandleObject,
        scope: HandleObject,
        obj_names: Vec<&str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        let mut arguments_value_vec: Vec<JSVal> = vec![];

        for arg_vf in args {
            arguments_value_vec.push(arg_vf.to_es_value(context));
        }

        rooted!(in(context) let mut rval = UndefinedValue());

        let res2: Result<(), EsErrorInfo> = es_utils::functions::call_obj_method_name(
            context,
            scope,
            obj_names,
            function_name,
            arguments_value_vec,
            &mut rval.handle_mut(),
        );

        if res2.is_ok() {
            return Ok(EsValueFacade::new_v(rt, context, global, rval.handle()));
        } else {
            return Err(res2.err().unwrap());
        }
    }

    /// call a method by name
    #[allow(dead_code)]
    fn call_method_name(
        &self,
        rt: &Runtime,
        context: *mut JSContext,
        global: HandleObject,
        scope: HandleObject,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        let mut arguments_value_vec: Vec<JSVal> = vec![];

        for arg_vf in args {
            arguments_value_vec.push(arg_vf.to_es_value(context));
        }

        rooted!(in(context) let mut rval = UndefinedValue());

        let res2: Result<(), EsErrorInfo> = es_utils::functions::call_method_name(
            context,
            scope,
            function_name,
            arguments_value_vec,
            &mut rval.handle_mut(),
        );

        if res2.is_ok() {
            return Ok(EsValueFacade::new_v(rt, context, global, rval.handle()));
        } else {
            return Err(res2.err().unwrap());
        }
    }

    /// use the jsapi objects in this runtime
    /// the consumer should be in the form of |rt: &Runtime, cx: *mut JSContext, global_handle: HandleObject| {}
    /// before calling the consumer the global obj is rooted and its Compartment is entered
    pub fn do_with_jsapi<C, R>(&self, consumer: C) -> R
    where
        C: FnOnce(&Runtime, *mut JSContext, HandleObject) -> R,
    {
        let rt = &self.runtime;
        let cx = rt.cx();
        let global = self.global_obj;

        rooted!(in (cx) let global_root = global);

        let ret;
        {
            trace!("do_with_jsapi _ac");
            let _ac = JSAutoCompartment::new(cx, global);
            trace!("do_with_jsapi consume");
            ret = consumer(rt, cx, global_root.handle());
        }
        ret
    }
}

thread_local! {
    pub static MODULE_CACHE: RefCell<LruCache<String, EsPersistentRooted>> = RefCell::new(init_cache());
}

fn init_cache() -> LruCache<String, EsPersistentRooted> {
    let ct = SM_RT.with(|sm_rt_rc| {
        let sm_rt = &*sm_rt_rc.borrow();
        sm_rt.clone_rtw_inner().module_cache_size.clone()
    });

    LruCache::new(ct)
}

/// native function used a import function for module loading
unsafe extern "C" fn import_module(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ != 2 {
        JS_ReportErrorASCII(
            context,
            b"import_module() requires exactly 2 arguments\0".as_ptr() as *const libc::c_char,
        );
        return false;
    }

    let arg_module = mozjs::rust::Handle::from_raw(args.get(0));
    let arg_specifier = mozjs::rust::Handle::from_raw(args.get(1));

    let file_name = es_utils::es_value_to_str(context, &*arg_specifier);

    // see if we have that module
    let cached: bool = MODULE_CACHE.with(|cache_rc| {
        let cache = &mut *cache_rc.borrow_mut();
        if let Some(mpr) = cache.get(&file_name) {
            trace!("found a cached module for {}", &file_name);
            // set rval here
            args.rval().set(ObjectValue(mpr.get()));
            return true;
        }
        false
    });
    if cached {
        return true;
    }

    // see if we got a module code loader
    let module_src = SM_RT.with(|sm_rt_rc| {
        let sm_rt = sm_rt_rc.borrow();
        let es_rt_wrapper_inner = sm_rt.clone_rtw_inner();
        if let Some(module_source_loader) = &es_rt_wrapper_inner.module_source_loader {
            return module_source_loader(file_name.as_str());
        }
        return format!("");
    });

    // is this my outer module or something?
    let _js_module_obj: *mut JSObject = arg_module.to_object();

    let compiled_mod_obj_res =
        es_utils::modules::compile_module(context, module_src.as_str(), file_name.as_str());

    if compiled_mod_obj_res.is_err() {
        let err = compiled_mod_obj_res.err().unwrap();
        let err_str = format!(
            "error loading module: at {}:{}:{} > {}\n",
            err.filename, err.lineno, err.column, err.message
        );
        JS_ReportErrorASCII(context, err_str.as_ptr() as *const libc::c_char);
        return false;
    }

    let compiled_module: *mut JSObject = compiled_mod_obj_res.ok().unwrap();

    MODULE_CACHE.with(|cache_rc| {
        trace!("caching module for {}", &file_name);
        let cache = &mut *cache_rc.borrow_mut();
        let mpr = EsPersistentRooted::new_from_obj(context, compiled_module);
        cache.put(file_name, mpr);
    });

    args.rval().set(ObjectValue(compiled_module));
    return true;
}

/// deprecated log function used for debugging, should be removed now that the native console obj is working
unsafe extern "C" fn log(context: *mut JSContext, argc: u32, vp: *mut mozjs::jsapi::Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ != 1 {
        JS_ReportErrorASCII(
            context,
            b"log() requires exactly 1 argument\0".as_ptr() as *const libc::c_char,
        );
        return false;
    }

    let arg1 = mozjs::rust::Handle::from_raw(args.get(0));
    let message = es_utils::es_value_to_str(context, &arg1.get());

    println!("__log: {} from thread {}", message, thread_id::get());

    args.rval().set(UndefinedValue());
    return true;
}

/// this function is called from script when the script invokes esses.invoke_rust_op
/// it is used to invoke native rust functions from script
/// based on the value fo the second param it may return synchronously, or return a new Promise object, or just run asynchronously
unsafe extern "C" fn invoke_rust_op(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    if args.argc_ < 1 {
        JS_ReportErrorASCII(
            context,
            b"invoke_rust_op() requires at least 1 argument: op_name\0".as_ptr()
                as *const libc::c_char,
        );
        return false;
    }

    let op_name_arg: HandleValue = mozjs::rust::Handle::from_raw(args.get(0));
    let op_name = es_utils::es_value_to_str(context, &op_name_arg.get());

    trace!("running rust-op {} with and {} args", op_name, args.argc_);

    // todo parse multiple args and stuff.. and scope... etc
    let mut args_vec: Vec<EsValueFacade> = Vec::new();

    let op_res: Result<EsValueFacade, String> = SM_RT.with(move |sm_rt_rc| {
        trace!("about to borrow sm_rt");
        let sm_rt_ref = sm_rt_rc.borrow();
        let sm_rt: &SmRuntime = sm_rt_ref.deref();

        let global = sm_rt.global_obj;
        let rt = &sm_rt.runtime;
        rooted!(in (context) let global_root = global);

        for x in 1..args.argc_ {
            let var_arg: HandleValue = mozjs::rust::Handle::from_raw(args.get(x));
            args_vec.push(EsValueFacade::new_v(
                rt,
                context,
                global_root.handle(),
                var_arg,
            ));
        }

        trace!("about to invoke op {} from sm_rt", op_name);
        sm_rt.invoke_op(op_name, args_vec)
    });

    // return stuff as JSVal
    let mut es_ret_val = UndefinedValue(); // dit is een Value
    if op_res.is_ok() {
        let op_res_esvf = op_res.unwrap();
        es_ret_val = op_res_esvf.to_es_value(context);
    } else {
        // todo report error to js?
        debug!("op failed with {}", op_res.err().unwrap());
        JS_ReportErrorASCII(context, b"op failed\0".as_ptr() as *const libc::c_char);
    }

    args.rval().set(es_ret_val);
    return true;
}

impl Drop for SmRuntime {
    fn drop(&mut self) {
        trace!("dropping SmRuntime in thread {}", thread_id::get());
        self.opt_es_rt_inner = None;
        trace!("dropping SmRuntime 2 in thread {}", thread_id::get());
    }
}

/// this function is called when servo needs to scheduale a callback function to be executed
/// asynchronously because a Promise was constructed
/// the callback obj is rooted and unrooted when dropped
/// the async job is fed to the microtaskmanager and invoked later
unsafe extern "C" fn enqueue_job(
    cx: *mut mozjs::jsapi::JSContext,
    job: mozjs::jsapi::Handle<*mut mozjs::jsapi::JSObject>,
    _allocation_site: mozjs::jsapi::Handle<*mut mozjs::jsapi::JSObject>,
    _obj3: mozjs::jsapi::Handle<*mut mozjs::jsapi::JSObject>,
    _data: *mut std::ffi::c_void,
) -> bool {
    wrap_panic(
        AssertUnwindSafe(move || {
            trace!("enqueue a job");

            let cb = PromiseJobCallback::new(cx, job.get());

            let task = move || {
                SM_RT.with(move |rc| {
                    trace!("running a job");

                    let sm_rt = &*rc.borrow();
                    sm_rt.do_with_jsapi(|_rt, cx, global| {
                        let call_res = cb.call(cx, global);
                        if call_res.is_err() {
                            debug!("job failed");
                            if let Some(err) = es_utils::report_es_ex(cx) {
                                panic!(
                                    "job failed {}:{}:{} -> {}",
                                    err.filename, err.lineno, err.column, err.message
                                );
                            }
                        }
                    })
                });
            };

            SM_RT.with(move |sm_rt_rc| {
                let sm_rt = &*sm_rt_rc.borrow();
                let esrtwi_opt = sm_rt.opt_es_rt_inner.as_ref().unwrap().upgrade();
                let esrtwi: Arc<EsRuntimeWrapperInner> = esrtwi_opt.unwrap();
                let tm = esrtwi.task_manager.clone();
                tm.add_task_from_worker(task);
            });

            true
        }),
        false,
    )
}

/// the code below was copied and altered from the servo project
/// https://github.com/servo/servo
/// so it falls under this LICENSE https://raw.githubusercontent.com/servo/servo/master/LICENSE
///

struct PromiseJobCallback {
    pub parent: CallbackFunction,
}

impl PromiseJobCallback {
    pub unsafe fn new(cx: *mut JSContext, a_callback: *mut JSObject) -> Rc<PromiseJobCallback> {
        let mut ret = Rc::new(PromiseJobCallback {
            parent: CallbackFunction::new(),
        });
        // Note: callback cannot be moved after calling init.
        match Rc::get_mut(&mut ret) {
            Some(ref mut callback) => callback.parent.init(cx, a_callback),
            None => unreachable!(),
        };
        ret
    }

    unsafe fn call(&self, cx: *mut JSContext, a_this_obj: HandleObject) -> Result<(), ()> {
        rooted!(in(cx) let mut rval = UndefinedValue());

        rooted!(in(cx) let callable = ObjectValue(self.parent.callback_holder().get()));
        rooted!(in(cx) let rooted_this = a_this_obj.get());
        let ok = JS_CallFunctionValue(
            cx,
            rooted_this.handle(),
            callable.handle(),
            &HandleValueArray {
                length_: 0 as ::libc::size_t,
                elements_: ptr::null_mut(),
            },
            rval.handle_mut(),
        );
        //maybe_resume_unwind();
        if !ok {
            return Err(());
        }

        Ok(())
    }
}

struct CallbackFunction {
    object: EsPersistentRooted,
}

impl CallbackFunction {
    /// Create a new `CallbackFunction` for this object.

    pub fn new() -> CallbackFunction {
        CallbackFunction {
            object: EsPersistentRooted::new(),
        }
    }

    /// Returns the underlying `CallbackObject`.
    pub fn callback_holder(&self) -> &EsPersistentRooted {
        &self.object
    }

    /// Initialize the callback function with a value.
    /// Should be called once this object is done moving.
    pub unsafe fn init(&mut self, cx: *mut JSContext, callback: *mut JSObject) {
        self.object.init(cx, callback);
    }
}

#[cfg(test)]
mod tests {
    use crate::es_utils;
    use crate::es_utils::EsErrorInfo;
    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_call_method_name() {
        //simple_logger::init().unwrap();

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(

                Box::new(|sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {
                        rooted!(in(cx) let mut rval = UndefinedValue());
                        let _eval_res = es_utils::eval(
                            rt,
                            global,
                            "this.test_func_1 = function test_func_1(a, b, c){return (a + '_' + b + '_' + c);};",
                            "test_call_method_name.es",
                            rval.handle_mut(),
                        );

                        let res = sm_rt.call_method_name(
                            rt,
                            cx,
                            global,
                            global,
                            "test_func_1",
                            vec![
                                EsValueFacade::new_str("abc".to_string()),
                                EsValueFacade::new_bool(true),
                                EsValueFacade::new_i32(123)
                            ],
                        );

                        if res.is_ok() {
                            let esvf = res.ok().unwrap();
                            return esvf.get_string().to_string();
                        } else {
                            let err = res.err().unwrap();
                            panic!("err {}", err.message);
                        }
                    })
                }

                ))
        });

        assert_eq!(res, "abc_true_123".to_string());
    }

    fn _test_import() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let import_res: Result<EsValueFacade, EsErrorInfo> = rt.eval_sync(
            "import {foo, bar} from 'test_module';\n\n'ok';",
            "test_import.es",
        );
        if import_res.is_err() {
            panic!("eval import failed: {}", import_res.err().unwrap().message);
        }
        let esvf = import_res.ok().unwrap();

        assert_eq!(esvf.get_string(), &"ok".to_string());
    }

    /// dynamic imports don't seem to be implemented in our version of JSAPI, so we'll skip this for now
    fn _test_dynamic_import() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let import_res: Result<EsValueFacade, EsErrorInfo> = rt.eval_sync(
            "import('test_module').then((answer) => {console.log('imported module: ' + JSON.stringify(answer));});\n\n'ok';",
            "test_dynamic_import.es",
        );
        if import_res.is_err() {
            panic!("eval import failed: {}", import_res.err().unwrap().message);
        }
        let esvf = import_res.ok().unwrap();

        assert_eq!(esvf.get_string(), &"ok".to_string());
    }

    #[test]
    fn test_call_method_obj_name() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(

                Box::new(|sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {


                        rooted!(in(cx) let mut rval = UndefinedValue());
                        let _eval_res = es_utils::eval(
                            rt,
                            global,
                            "this.myobj = {sub: {}};myobj.sub.test_func_1 = function test_func_1(a, b, c){return (a + '_' + b + '_' + c);};",
                            "test_call_method_name.es",
                            rval.handle_mut(),
                        );

                        let res = sm_rt.call_obj_method_name(
                            rt,
                            cx,
                            global,
                            global,
                            vec!["myobj", "sub"],
                            "test_func_1",
                            vec![
                                EsValueFacade::new_str("abc".to_string()),
                                EsValueFacade::new_bool(true),
                                EsValueFacade::new_i32(123)
                            ],
                        );

                        if res.is_ok() {
                            let esvf = res.ok().unwrap();
                            return esvf.get_string().to_string();
                        } else {
                            let err = res.err().unwrap();
                            panic!("err {}", err.message);
                        }
                    })
                })
                )
        });

        assert_eq!(res, "abc_true_123".to_string());
    }

    // used for testing with gc_zeal opts
    // #[test]
    fn _test_simple_inner() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        rt.do_with_inner(|inner| {
            for _x in 0..5000 {
                inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {
                        rooted!(in (cx) let mut ret_val = UndefinedValue());

                        es_utils::eval(rt, global, "({a: 1});", "test.es", ret_val.handle_mut())
                            .ok()
                            .unwrap();
                    })
                }));
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    }
}
