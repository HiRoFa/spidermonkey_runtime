use crate::es_utils;
use crate::es_utils::EsErrorInfo;
use crate::esruntimewrapper::EsRuntimeWrapper;

use crate::esvaluefacade::EsValueFacade;
use log::{debug, trace};

use mozjs::jsapi::CallArgs;
use mozjs::jsapi::CompartmentOptions;
use mozjs::jsapi::Heap;
use mozjs::jsapi::JSAutoCompartment;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::JS_NewGlobalObject;
use mozjs::jsapi::JS_ReportErrorASCII;
use mozjs::jsapi::OnNewGlobalHookOption;
use mozjs::jsapi::SetEnqueuePromiseJobCallback;
use mozjs::jsapi::JS::HandleValueArray;

use mozjs::jsapi::{AddRawValueRoot, RemoveRawValueRoot};
use mozjs::rust::wrappers::JS_CallFunctionValue;
use mozjs::rust::{HandleObject, HandleValue, JSEngine};
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};

use mozjs::jsval::{JSVal, ObjectValue, UndefinedValue};
use std::cell::RefCell;
use std::collections::HashMap;

use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use mozjs::panic::wrap_panic;
use std::ffi::CString;
use std::panic::AssertUnwindSafe;
use std::ptr;
use std::rc::Rc;
use std::str;
use std::sync::{Arc, Weak};

/// the type for registering rust_ops in the script engine
pub type OP = Box<dyn Fn(&SmRuntime, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send>;

/// wrapper for the SpiderMonkey runtime
pub struct SmRuntime {
    pub(crate) runtime: mozjs::rust::Runtime,
    pub(crate) global_obj: *mut JSObject,
    _ac: JSAutoCompartment,
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

        rooted!(in(context) let global_root = global_obj);
        let global = global_root.handle();
        let ac = JSAutoCompartment::new(context, global.get());

        unsafe {
            let function = JS_DefineFunction(
                context,
                global.into(),
                b"__log\0".as_ptr() as *const libc::c_char,
                Some(log),
                1,
                0,
            );
            assert!(!function.is_null());
            let function = JS_DefineFunction(
                context,
                global.into(),
                b"__invoke_rust_op\0".as_ptr() as *const libc::c_char,
                Some(invoke_rust_op),
                1,
                0,
            );
            assert!(!function.is_null());

            // this tells JSAPI how to schedule jobs for Promises
            SetEnqueuePromiseJobCallback(context, Some(enqueue_job), ptr::null_mut());
        }

        SmRuntime {
            runtime,
            global_obj,
            _ac: ac,
            op_container: HashMap::new(),
            opt_es_rt_inner: None,
        }
    }

    /// call a function by name, the function needs to be defined on the root of the global scope
    pub fn call(
        &self,
        func_name: &str,
        arguments: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        let runtime: &mozjs::rust::Runtime = &self.runtime;
        let context = runtime.cx();

        rooted!(in(context) let global_root = self.global_obj);
        let global = global_root.handle();

        trace!("smrt.call {} in thread {}", func_name, thread_id::get());

        es_utils::call_method_name(context, *global, func_name, arguments)
    }

    /// eval a piece of script
    /// please note that in order to return a value form script you need to eval with 'return value;'
    /// and not just 'value;' because the script to be evaluated is wrapped in a function
    /// this is needed for now because we do some preprocessing for when Promises are passed to rust
    /// this will change in the future
    pub fn eval(&self, eval_code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        debug!("smrt.eval {} in thread {}", eval_code, thread_id::get());

        // todo remove outer function
        let wrapped_code = format!(
            "\
             (function(){{\
             let retVal = function(){{\
             {}\
             }}.apply(this);\
             retVal = esses.prepValForOutputToRust(retVal);\
             return retVal;\
             }}).apply(this);\
             ",
            eval_code
        );

        es_utils::eval(
            &self.runtime,
            self.global_obj,
            wrapped_code.as_str(),
            file_name,
        )
    }

    /// run the cleanup function and run the garbage collector
    /// this also fires a pre-cleanup event in script so scripts can do a cleanup before the garbage collector runs
    pub fn cleanup(&self) {
        trace!("cleaning up sm_rt");

        // todo, should this return a list of available scopes in the runtime? (eventually stored in the esruntimeInner)

        let cleanup_res = self.call("_esses_cleanup", Vec::new());
        assert!(cleanup_res.is_ok());

        let runtime: &mozjs::rust::Runtime = &self.runtime;
        let context = runtime.cx();

        rooted!(in(context) let global_root = self.global_obj);
        // not used but important for letting gc run without cleaning up the global object
        let _global = global_root.handle();

        es_utils::gc(context);
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

    // todo, runtime_id_arg will now be runtime_id, which we can then get from ESENGINE
    let op_name_arg: HandleValue = mozjs::rust::Handle::from_raw(args.get(0));
    let op_name = es_utils::es_value_to_str(context, &op_name_arg.get());

    trace!("running rust-op {} with and {} args", op_name, args.argc_);

    // todo parse multiple args and stuff.. and scope... etc
    let mut args_vec: Vec<EsValueFacade> = Vec::new();
    for x in 1..args.argc_ {
        let var_arg: HandleValue = mozjs::rust::Handle::from_raw(args.get(x));
        args_vec.push(EsValueFacade::new(context, var_arg));
    }

    let op_res: Result<EsValueFacade, String> = SM_RT.with(move |sm_rt_rc| {
        trace!("about to borrow sm_rt");
        let sm_rt = sm_rt_rc.borrow();
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
                    let sm_rt = &*rc.borrow();
                    let cx = sm_rt.runtime.cx();
                    let global = sm_rt.global_obj;
                    rooted!(in(cx) let global_root = global);
                    cb.call(cx, global_root.handle()).unwrap();
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

        rooted!(in(cx) let callable = ObjectValue(self.parent.callback_holder().callback.get()));
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
    object: CallbackObject,
}

impl CallbackFunction {
    /// Create a new `CallbackFunction` for this object.

    pub fn new() -> CallbackFunction {
        CallbackFunction {
            object: CallbackObject::new(),
        }
    }

    /// Returns the underlying `CallbackObject`.
    pub fn callback_holder(&self) -> &CallbackObject {
        &self.object
    }

    /// Initialize the callback function with a value.
    /// Should be called once this object is done moving.
    pub unsafe fn init(&mut self, cx: *mut JSContext, callback: *mut JSObject) {
        self.object.init(cx, callback);
    }
}

pub struct CallbackObject {
    /// The underlying `JSObject`.
    callback: Heap<*mut JSObject>,
    permanent_js_root: Heap<JSVal>,
}

impl Default for CallbackObject {
    fn default() -> CallbackObject {
        CallbackObject::new()
    }
}

impl CallbackObject {
    fn new() -> CallbackObject {
        CallbackObject {
            callback: Heap::default(),
            permanent_js_root: Heap::default(),
        }
    }

    pub fn get(&self) -> *mut JSObject {
        self.callback.get()
    }

    #[allow(unsafe_code)]
    unsafe fn init(&mut self, cx: *mut JSContext, callback: *mut JSObject) {
        self.callback.set(callback);
        self.permanent_js_root.set(ObjectValue(callback));
        let c_str = CString::new("CallbackObject::root").unwrap();
        assert!(AddRawValueRoot(
            cx,
            self.permanent_js_root.get_unsafe(),
            c_str.as_ptr() as *const i8
        ));
    }
}

impl Drop for CallbackObject {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe {
            let cx = Runtime::get();
            RemoveRawValueRoot(cx, self.permanent_js_root.get_unsafe());
        }
    }
}

impl PartialEq for CallbackObject {
    fn eq(&self, other: &CallbackObject) -> bool {
        self.callback.get() == other.callback.get()
    }
}
