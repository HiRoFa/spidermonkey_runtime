use crate::esruntimeinner::EsRuntimeInner;
use crate::esvaluefacade::EsValueFacade;
use crate::jsapi_utils;
use crate::jsapi_utils::rooting::EsPersistentRooted;
use crate::jsapi_utils::EsErrorInfo;
use crate::utils::AutoIdMap;

use log::{debug, trace};
use lru::LruCache;

use mozjs::glue::{CreateJobQueue, JobQueueTraps};
use mozjs::jsapi::CallArgs;
use mozjs::jsapi::Handle as RawHandle;
use mozjs::jsapi::HandleValue as RawHandleValue;
use mozjs::jsapi::JSAutoRealm;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSString;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_NewGlobalObject;
use mozjs::jsapi::JS_ReportErrorASCII;
use mozjs::jsapi::OnNewGlobalHookOption;
use mozjs::jsapi::SetJobQueue;
use mozjs::jsapi::SetModuleResolveHook;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::{NullValue, ObjectValue, UndefinedValue};
use mozjs::panic::wrap_panic;
use mozjs::rust::wrappers::JS_CallFunctionValue;
use mozjs::rust::HandleObject;
use mozjs::rust::RealmOptions;
use mozjs::rust::Runtime;
use mozjs::rust::SIMPLE_GLOBAL_CLASS;

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_void;

use std::ptr;
use std::rc::Rc;
use std::str;
use std::sync::{Arc, Weak};

/// the type for registering rust_ops in the script engine
// todo remove
pub type OP =
    Arc<dyn Fn(&EsRuntimeInner, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + Sync>;

pub type GlobalOp = dyn Fn(*mut JSContext, CallArgs) -> bool + Send + 'static;

/// wrapper for the SpiderMonkey runtime, this struct only lives as a thread_local in the worker
/// thread of an EsRuntime
/// since it can only be accessed from that thread all methods here are sync
pub struct SmRuntime {
    runtime: mozjs::rust::Runtime,
    global_obj: *mut JSObject,
    // todo remove
    op_container: HashMap<String, OP>,
    pub(crate) opt_esrt_inner: Option<Weak<EsRuntimeInner>>,
}

thread_local! {
    /// the thread-local SpiderMonkeyRuntime
    /// this only exists for the worker thread of the MicroTaskManager
    pub(crate) static SM_RT: RefCell<SmRuntime> = RefCell::new(SmRuntime::new());
    static GLOBAL_OPS: RefCell<HashMap<&'static str, Box<GlobalOp>>> = RefCell::new(HashMap::new());
}

impl SmRuntime {
    pub fn clone_esrt_inner(&self) -> Arc<EsRuntimeInner> {
        self.opt_esrt_inner
            .as_ref()
            .expect("not initialized yet")
            .upgrade()
            .expect("parent EsRuntimeInner was dropped")
    }

    pub fn clone_current_esrt_inner_arc() -> Arc<EsRuntimeInner> {
        SM_RT.with(|sm_rt_rc| {
            let sm_rt = &*sm_rt_rc.borrow();
            sm_rt.clone_esrt_inner()
        })
    }

    /// add a function to the global object
    /// this function will be callable from javascript just by using func_name();
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use mozjs::jsval::Int32Value;
    /// use mozjs::jsapi::CallArgs;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.do_in_es_runtime_thread_sync(|sm_rt| {
    ///     sm_rt.add_global_function("my_function", |_cx, args: CallArgs| {
    ///         // impl method here
    ///         args.rval().set(Int32Value(480));
    ///         true
    ///     });
    /// });
    /// let esvf = rt.eval_sync("my_function();", "test_add_global_function_example.es").ok().expect("test_add_global_function_example failed");
    /// assert_eq!(esvf.get_i32(), &480);
    /// ```
    pub fn add_global_function<F>(&self, name: &'static str, func: F)
    where
        F: Fn(*mut JSContext, CallArgs) -> bool + Send + 'static,
    {
        GLOBAL_OPS.with(move |global_ops_rc| {
            let global_ops = &mut *global_ops_rc.borrow_mut();
            global_ops.insert(name, Box::new(func));
        });

        self.do_with_jsapi(|_rt, cx, global| {
            // reg function
            jsapi_utils::functions::define_native_function(
                cx,
                global,
                name,
                Some(global_op_native_method),
            );
        })
    }

    /// construct a new SmRuntime, this should only be called from the workerthread of the MicroTaskManager
    /// here we actualy construct a new Runtime
    fn new() -> Self {
        debug!("init SmRuntime {}", thread_id::get());

        let runtime = mozjs::rust::Runtime::new(crate::enginehandleproducer::produce());

        let context = runtime.cx();
        let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
        let c_option = RealmOptions::default();

        let global_obj;

        unsafe {
            global_obj = JS_NewGlobalObject(
                context,
                &SIMPLE_GLOBAL_CLASS,
                ptr::null_mut(),
                h_option,
                &*c_option,
            );
        }

        let mut ret = SmRuntime {
            runtime,
            global_obj,
            op_container: HashMap::new(),
            opt_esrt_inner: None,
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
                b"__invoke_rust_op_sync\0".as_ptr() as *const libc::c_char,
                Some(invoke_rust_op_sync),
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

        let job_queue = unsafe { CreateJobQueue(&JOB_QUEUE_TRAPS, ptr::null_mut()) };

        self.do_with_jsapi(|_rt, cx, _global| unsafe {
            SetJobQueue(cx, job_queue);
        });
    }

    fn init_import_callbacks(&mut self) {
        // this tells the runtime how to resolve modules
        let js_runtime = &mut self.runtime.rt();
        self.do_with_jsapi(|_rt, _cx, _global| {
            unsafe { SetModuleResolveHook(*js_runtime, Some(import_module)) };
        });
    }

    // call a function by name
    // todo this should not be here, SmRuntime should not return EsValueFacades
    pub fn call(
        &self,
        obj_names: Vec<&str>,
        func_name: &str,
        arguments: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_jsapi(|rt, _cx, global| {
            trace!("smrt.call {} in thread {}", func_name, thread_id::get());

            self.call_obj_method_name(rt, global, global, obj_names, func_name, arguments)
        })
    }

    /// load and execute a script module
    pub fn load_module(&self, module_src: &str, module_file_name: &str) -> Result<(), EsErrorInfo> {
        trace!(
            "smrt.load_module {} in thread {}",
            module_file_name,
            thread_id::get()
        );

        self.do_with_jsapi(|_rt, cx, _global| {
            let load_res = jsapi_utils::modules::compile_module(cx, module_src, module_file_name);

            if let Some(err) = load_res.err() {
                return Err(err);
            }

            Ok(())
        })
    }

    /// eval a piece of script and return the result as a EsValueFacade
    // todo, this should not return an EsValueFacade, refactor to rval
    pub fn eval(&self, eval_code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("smrt.eval {} in thread {}", file_name, thread_id::get());

        self.do_with_jsapi(|rt, cx, global| {
            rooted!(in (cx) let mut rval = UndefinedValue());
            let eval_res: Result<(), EsErrorInfo> =
                jsapi_utils::eval(rt, global, eval_code, file_name, rval.handle_mut());

            if eval_res.is_ok() {
                Ok(EsValueFacade::new_v(rt, cx, global, rval.handle()))
            } else {
                Err(eval_res.err().unwrap())
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
                jsapi_utils::eval(rt, global, eval_code, file_name, rval.handle_mut());

            if eval_res.is_ok() {
                Ok(())
            } else {
                Err(eval_res.err().unwrap())
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
                // run esses.cleanup();
                let cleanup_res = jsapi_utils::functions::call_obj_method_name(
                    cx,
                    global,
                    vec!["esses"],
                    "cleanup",
                    vec![],
                    ret_val.handle_mut(),
                );
                if cleanup_res.is_err() {
                    let err = cleanup_res.err().unwrap();
                    log::error!(
                        "cleanup failed: {}:{}:{} -> {}",
                        err.filename,
                        err.lineno,
                        err.column,
                        err.message
                    );
                }
            }
            trace!("running gc cleanup / 2");
        });
        self.do_with_jsapi(|_rt, cx, _global| {
            trace!("running gc");
            jsapi_utils::gc(cx);
            trace!("running gc / 2");
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

        let rt = self.clone_esrt_inner();
        op(rt.borrow(), args)
    }

    /// call a method by name on an object by name
    /// e.g. esses.cleanup() can be called by calling
    /// call_obj_method_name(cx, glob, vec!["esses"], "cleanup", vec![]);
    #[allow(dead_code)]
    fn call_obj_method_name(
        &self,
        rt: &Runtime,
        global: HandleObject,
        scope: HandleObject,
        obj_names: Vec<&str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("sm_rt.call_obj_method_name({}, ...)", function_name);

        let context = rt.cx();

        rooted!(in(context) let mut rval = UndefinedValue());
        do_with_rooted_esvf_vec(context, args, |hva| {
            let res2: Result<(), EsErrorInfo> = jsapi_utils::functions::call_obj_method_name2(
                context,
                scope,
                obj_names,
                function_name,
                hva,
                rval.handle_mut(),
            );

            if res2.is_ok() {
                Ok(EsValueFacade::new_v(rt, context, global, rval.handle()))
            } else {
                Err(res2.err().unwrap())
            }
        })
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
        rooted!(in(context) let mut rval = UndefinedValue());

        do_with_rooted_esvf_vec(context, args, |hva| {
            let res2: Result<(), EsErrorInfo> = jsapi_utils::functions::call_method_name2(
                context,
                scope,
                function_name,
                hva,
                rval.handle_mut(),
            );

            if res2.is_ok() {
                Ok(EsValueFacade::new_v(rt, context, global, rval.handle()))
            } else {
                Err(res2.err().unwrap())
            }
        })
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
            let _ac = JSAutoRealm::new(cx, global);
            trace!("do_with_jsapi consume");
            ret = consumer(rt, cx, global_root.handle());
        }
        ret
    }
}

unsafe extern "C" fn global_op_native_method(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    // todo get name from callee, get global op, invoke

    let args = CallArgs::from_vp(vp, argc);
    let callee: *mut JSObject = args.callee();
    let prop_name_res = crate::jsapi_utils::objects::get_es_obj_prop_val_as_string(
        cx,
        HandleObject::from_marked_location(&callee),
        "name",
    );
    if let Ok(prop_name) = prop_name_res {
        return GLOBAL_OPS.with(|global_ops_rc| {
            let global_ops = &*global_ops_rc.borrow();
            let boxed_op = global_ops
                .get(prop_name.as_str())
                .expect("could not find op");
            boxed_op(cx, args)
        });
    }

    false
}

thread_local! {
// store epr in Box because https://doc.servo.org/mozjs_sys/jsgc/struct.Heap.html#method.boxed
    static OBJECT_CACHE: RefCell<AutoIdMap<EsPersistentRooted>> = RefCell::new(AutoIdMap::new());
}

pub(crate) fn do_with_rooted_esvf_vec<R, C>(
    context: *mut JSContext,
    vec: Vec<EsValueFacade>,
    consumer: C,
) -> R
where
    C: FnOnce(HandleValueArray) -> R,
{
    trace!("sm_rt::do_with_rooted_esvf_vec, vec_len={}", vec.len());

    auto_root!(in (context) let mut values = vec![]);

    for esvf in vec {
        values.push(esvf.to_es_value(context));
    }

    trace!("sm_rt::do_with_rooted_esvf_vec, init hva");
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*values) };
    // root the hva itself
    trace!("sm_rt::do_with_rooted_esvf_vec, root hva");
    rooted!(in(context) let _argument_object = unsafe { JS_NewArrayObject(context, &arguments_value_array) });
    trace!("sm_rt::do_with_rooted_esvf_vec, run consumer");
    consumer(arguments_value_array)
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn register_cached_object(context: *mut JSContext, obj: *mut JSObject) -> usize {
    let mut epr = EsPersistentRooted::default();
    unsafe { epr.init(context, obj) };
    OBJECT_CACHE.with(|object_cache_rc| {
        let map = &mut *object_cache_rc.borrow_mut();

        let id = map.insert(epr);

        trace!("cache obj with id {}", id);

        id
    })
}

pub fn do_with_cached_object<C, R>(id: usize, consumer: C) -> R
where
    C: Fn(&EsPersistentRooted) -> R,
{
    OBJECT_CACHE.with(|object_cache_rc| {
        let map = &mut *object_cache_rc.borrow_mut();
        if let Some(epr) = map.get(&id) {
            consumer(epr)
        } else {
            panic!("no such id");
        }
    })
}

pub fn consume_cached_object(id: usize) -> EsPersistentRooted {
    trace!("consume cached obj with id {}", id);
    OBJECT_CACHE.with(|object_cache_rc| {
        let map = &mut *object_cache_rc.borrow_mut();
        map.remove(&id)
    })
}

thread_local! {
// store epr in Box because https://doc.servo.org/mozjs_sys/jsgc/struct.Heap.html#method.boxed
    static MODULE_CACHE: RefCell<LruCache<String, EsPersistentRooted>> = RefCell::new(init_cache());
}

fn init_cache() -> LruCache<String, EsPersistentRooted> {
    let ct = SM_RT.with(|sm_rt_rc| {
        let sm_rt = &*sm_rt_rc.borrow();
        sm_rt.clone_esrt_inner().module_cache_size
    });

    LruCache::new(ct)
}

/// native function used a import function for module loading
unsafe extern "C" fn import_module(
    cx: *mut JSContext,
    _reference_private: RawHandleValue,
    specifier: RawHandle<*mut JSString>,
) -> *mut JSObject {
    let file_name = jsapi_utils::es_jsstring_to_string(cx, *specifier);

    // see if we have that module
    let cached: Option<*mut JSObject> = MODULE_CACHE.with(|cache_rc| {
        let cache = &mut *cache_rc.borrow_mut();
        if let Some(mpr) = cache.get(&file_name) {
            trace!("found a cached module for {}", &file_name);
            // set rval here
            return Some(mpr.get());
        }
        None
    });
    if let Some(c) = cached {
        return c;
    };

    // see if we got a module code loader
    let module_src = SM_RT.with(|sm_rt_rc| {
        let sm_rt = sm_rt_rc.borrow();
        let es_rt_inner = sm_rt.clone_esrt_inner();
        if let Some(module_source_loader) = &es_rt_inner.module_source_loader {
            return module_source_loader(file_name.as_str());
        }
        return format!("");
    });

    let compiled_mod_obj_res =
        jsapi_utils::modules::compile_module(cx, module_src.as_str(), file_name.as_str());

    if compiled_mod_obj_res.is_err() {
        let err = compiled_mod_obj_res.err().unwrap();
        let err_str = format!(
            "error loading module: at {}:{}:{} > {}\n",
            err.filename, err.lineno, err.column, err.message
        );
        JS_ReportErrorASCII(cx, err_str.as_ptr() as *const libc::c_char);
        debug!("error loading module, returning null: {}", &err_str);
        return *ptr::null_mut::<*mut JSObject>();
    }

    let compiled_module: *mut JSObject = compiled_mod_obj_res.ok().unwrap();

    MODULE_CACHE.with(|cache_rc| {
        trace!("caching module for {}", &file_name);
        let cache = &mut *cache_rc.borrow_mut();
        let mut mpr = EsPersistentRooted::default();
        mpr.init(cx, compiled_module);
        cache.put(file_name, mpr);
    });

    compiled_module
}

/// this function is called from script when the script invokes esses.invoke_rust_op
/// it is used to invoke native rust functions from script
unsafe extern "C" fn invoke_rust_op(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    let op_res: Result<EsValueFacade, String> = invoke_rust_op_esvf(context, argc, vp, true);

    // return stuff as JSVal
    if op_res.is_ok() {
        let op_res_esvf = op_res.ok().unwrap();
        let es_ret_val = op_res_esvf.to_es_value(context);
        args.rval().set(es_ret_val);
    } else {
        // report error to js?
        debug!("op failed with {}", op_res.err().unwrap());
        JS_ReportErrorASCII(context, b"op failed\0".as_ptr() as *const libc::c_char);
    }
    true
}

/// this function is called from script when the script invokes esses.invoke_rust_op
/// it is used to invoke native rust functions from script
unsafe extern "C" fn invoke_rust_op_sync(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    let op_res: Result<EsValueFacade, String> = invoke_rust_op_esvf(context, argc, vp, false);

    // return stuff as JSVal
    if op_res.is_ok() {
        let op_res_esvf = op_res.ok().unwrap();
        let es_ret_val = op_res_esvf.to_es_value(context);
        args.rval().set(es_ret_val);
    } else {
        // report error to js?
        debug!("op failed with {}", op_res.err().unwrap());
        JS_ReportErrorASCII(context, b"op failed\0".as_ptr() as *const libc::c_char);
    }
    true
}

/// this function is called from script when the script invokes esses.invoke_rust_op
/// it is used to invoke native rust functions from script
fn invoke_rust_op_esvf(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
    as_promise: bool,
) -> Result<EsValueFacade, String> {
    let args = unsafe { CallArgs::from_vp(vp, argc) };

    if args.argc_ < 1 {
        return Err("invoke_rust_op requires at least one arg: op_name".to_string());
    }

    let op_name_arg: mozjs::rust::HandleValue =
        unsafe { mozjs::rust::Handle::from_raw(args.get(0)) };
    let op_name = jsapi_utils::es_value_to_str(context, op_name_arg.get())
        .ok()
        .unwrap();

    trace!("running rust-op {} with and {} args", op_name, args.argc_);

    // todo root these
    let mut args_vec: Vec<EsValueFacade> = Vec::new();

    SM_RT.with(move |sm_rt_rc| {
        trace!("about to borrow sm_rt");
        let sm_rt = &*sm_rt_rc.borrow();

        let rt = &sm_rt.runtime;
        rooted!(in (context) let global_root = sm_rt.global_obj);

        for x in 1..args.argc_ {
            let var_arg: mozjs::rust::HandleValue =
                unsafe { mozjs::rust::Handle::from_raw(args.get(x)) };
            args_vec.push(EsValueFacade::new_v(
                rt,
                context,
                global_root.handle(),
                var_arg,
            ));
        }

        if as_promise {
            let rt = sm_rt.clone_esrt_inner();
            let op = sm_rt
                .op_container
                .get(&op_name)
                .expect("no such op")
                .clone();
            Ok(EsValueFacade::new_promise(move || {
                op(rt.borrow(), args_vec)
            }))
        } else {
            sm_rt.invoke_op(op_name, args_vec)
        }
    })
}

impl Drop for SmRuntime {
    fn drop(&mut self) {
        trace!("dropping SmRuntime in thread {}", thread_id::get());
        self.opt_esrt_inner = None;
        trace!("dropping SmRuntime 2 in thread {}", thread_id::get());
    }
}

/// this function is called when servo needs to schedule a callback function to be executed
/// asynchronously because a Promise was constructed
/// the callback obj is rooted and unrooted when dropped
/// the async job is fed to the microtaskmanager and invoked later
unsafe extern "C" fn enqueue_promise_job(
    _extra: *const c_void,
    cx: *mut JSContext,
    _promise: mozjs::jsapi::HandleObject,
    job: mozjs::jsapi::HandleObject,
    _allocation_site: mozjs::jsapi::HandleObject,
    _incumbent_global: mozjs::jsapi::HandleObject,
) -> bool {
    let mut result = false;
    wrap_panic(&mut || {
        trace!("enqueue a job");

        let cb = PromiseJobCallback::new(cx, job.get());

        let task = move || {
            SM_RT.with(move |rc| {
                trace!("running a job");

                let sm_rt = &*rc.borrow();

                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                    trace!("rooting null");
                    rooted!(in (cx) let null_root = NullValue().to_object_or_null());

                    trace!("calling cb.call");
                    let call_res = cb.call(cx, null_root.handle());
                    trace!("checking cb.call res");
                    if call_res.is_err() {
                        debug!("job failed");
                        if let Some(err) = jsapi_utils::report_es_ex(cx) {
                            panic!(
                                "job failed {}:{}:{} -> {}",
                                err.filename, err.lineno, err.column, err.message
                            );
                        }
                    }
                });
                trace!("job ran ok");
            });
        };

        SM_RT.with(move |sm_rt_rc| {
            let sm_rt = &*sm_rt_rc.borrow();
            let esrt_inner_opt = sm_rt.opt_esrt_inner.as_ref().unwrap().upgrade();
            let esrt_inner: Arc<EsRuntimeInner> = esrt_inner_opt.unwrap();
            let tm = esrt_inner.task_manager.clone();
            tm.add_task_from_worker(task);
        });
        result = true
    });
    result
}

/// the code below was copied and altered from the servo project
/// https://github.com/servo/servo
/// so it falls under this LICENSE https://raw.githubusercontent.com/servo/servo/master/LICENSE
///
///
#[allow(unsafe_code)]
unsafe extern "C" fn get_incumbent_global(_: *const c_void, _: *mut JSContext) -> *mut JSObject {
    let mut result = ptr::null_mut();
    trace!("get_incumbent_global called");
    wrap_panic(&mut || {
        // todo what to do here

        result = SM_RT.with(|sm_rt_rc| {
            let sm_rt = &*sm_rt_rc.borrow();
            sm_rt.global_obj
        });
    });
    result
}

#[allow(unsafe_code)]
unsafe extern "C" fn empty(_extra: *const c_void) -> bool {
    trace!("empty called");
    let mut result = false;
    wrap_panic(&mut || {
        result = SM_RT.with(|sm_rt_rc| {
            let sm_rt = &*sm_rt_rc.borrow();
            sm_rt.clone_esrt_inner().task_manager.is_empty()
        })
    });
    result
}

static JOB_QUEUE_TRAPS: JobQueueTraps = JobQueueTraps {
    getIncumbentGlobal: Some(get_incumbent_global),
    enqueuePromiseJob: Some(enqueue_promise_job),
    empty: Some(empty),
};

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
        trace!("PromiseJobCallback.call / 1");
        rooted!(in(cx) let mut rval = UndefinedValue());
        trace!("PromiseJobCallback.call / 2");
        rooted!(in(cx) let callable = ObjectValue(self.parent.callback_holder().get()));
        trace!("PromiseJobCallback.call / 3");
        //rooted!(in(cx) let rooted_this = a_this_obj.get());
        let ok = JS_CallFunctionValue(
            cx,
            a_this_obj,
            callable.handle(),
            &HandleValueArray {
                length_: 0 as ::libc::size_t,
                elements_: ptr::null_mut(),
            },
            rval.handle_mut(),
        );
        trace!("PromiseJobCallback.call / 4");
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
    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils;
    use crate::jsapi_utils::EsErrorInfo;
    use crate::spidermonkeyruntimewrapper::{do_with_rooted_esvf_vec, SmRuntime};
    use log::trace;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_call_method_name() {
        log::info!("test: test_call_method_name");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(

                |sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {
                        rooted!(in(cx) let mut rval = UndefinedValue());
                        let _eval_res = jsapi_utils::eval(
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
                            esvf.get_string().to_string()
                        } else {
                            let err = res.err().unwrap();
                            panic!("err {}", err.message);
                        }
                    })
                }

                )
        });

        assert_eq!(res, "abc_true_123".to_string());
    }

    fn _test_import() {
        log::info!("test: test_import");
        let rt = crate::esruntime::tests::TEST_RT.clone();
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
        log::info!("test: test_dynamic_import");
        let rt = crate::esruntime::tests::TEST_RT.clone();
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
        log::info!("test: test_call_method_obj_name");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(

                Box::new(|sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {


                        rooted!(in(cx) let mut rval = UndefinedValue());
                        let _eval_res = jsapi_utils::eval(
                            rt,
                            global,
                            "this.myobj = {sub: {}};myobj.sub.test_func_1 = function test_func_1(a, b, c){return (a + '_' + b + '_' + c);};",
                            "test_call_method_name.es",
                            rval.handle_mut(),
                        );

                        let res = sm_rt.call_obj_method_name(
                            rt,
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
                            esvf.get_string().to_string()
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
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_with_inner(|inner| {
            for _x in 0..5000 {
                inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                    sm_rt.do_with_jsapi(|rt, cx, global| {
                        rooted!(in (cx) let mut ret_val = UndefinedValue());

                        jsapi_utils::eval(rt, global, "({a: 1});", "test.es", ret_val.handle_mut())
                            .ok()
                            .unwrap();
                    })
                }));
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    }

    #[test]
    fn test_hva() {
        log::info!("test: test_hva");
        use mozjs::jsapi::HandleValueArray;

        let rt = crate::esruntime::tests::TEST_RT.clone();
        let ret = rt.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in (cx) let mut func_root = UndefinedValue());
                rt.evaluate_script(
                    global,
                    "(function(a, b, c, d){return [a, b, c, d].join('-');});",
                    "test_hva.es",
                    0,
                    func_root.handle_mut(),
                )
                .ok()
                .unwrap();

                let mut ret = "".to_string();
                for _x in 0..100 {
                    trace!("test_hva_loop");
                    let args = vec![
                        EsValueFacade::new_i32(1),
                        EsValueFacade::new_str("abc".to_string()),
                        EsValueFacade::new_i32(3),
                        EsValueFacade::new_str("def".to_string()),
                    ];
                    trace!("test_hva_loop / 2");
                    ret = do_with_rooted_esvf_vec(cx, args, |hva: HandleValueArray| {
                        rooted!(in (cx) let mut rval = UndefinedValue());
                        jsapi_utils::functions::call_method_value2(
                            cx,
                            global,
                            func_root.handle(),
                            hva,
                            rval.handle_mut(),
                        )
                        .ok()
                        .unwrap();
                        jsapi_utils::es_value_to_str(cx, *rval).ok().unwrap()
                    });
                    trace!("test_hva_loop / 3");
                }
                ret
            })
        });
        assert_eq!(ret.as_str(), "1-abc-3-def");
    }
}
