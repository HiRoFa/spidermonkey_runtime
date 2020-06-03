use std::{str, thread};

use std::sync::{Arc, Weak};

use crate::es_sys_scripts;
use crate::features;

use crate::es_utils::EsErrorInfo;
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use crate::esvaluefacade::EsValueFacade;

use crate::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use crate::taskmanager::TaskManager;
use std::cell::RefCell;
use std::time::Duration;

lazy_static! {
    /// a static Multithreaded taskmanager used to run rust ops async and multithreaded
    static ref HELPER_TASKS: Arc<TaskManager> = Arc::new(TaskManager::new(num_cpus::get()));
}

/// the EsRuntimeWrapper is a facade that adds all script todo's to the SmRuntimeWrapper's MicroTaskManager so they are invoked in a single worker thread
/// you can wait for those tasks to complete by calling the _sync variants of the public methods here
pub struct EsRuntimeWrapper {
    inner: Arc<EsRuntimeWrapperInner>,
}

pub type ModuleCodeLoader = dyn Fn(&str) -> String + Send + Sync + 'static;

impl EsRuntimeWrapper {
    pub fn builder() -> EsRuntimeWrapperBuilder {
        EsRuntimeWrapperBuilder::new()
    }

    pub(crate) fn new_inner(inner: EsRuntimeWrapperInner) -> Self {
        let arc_inner = Arc::new(inner);
        let sm_ref_inner: Weak<EsRuntimeWrapperInner> = Arc::downgrade(&arc_inner);
        let rt = EsRuntimeWrapper { inner: arc_inner };

        // pass arc around inner to sm_rt thread

        rt.inner.task_manager.exe_task(move || {
            // todo this should also be in init_info

            crate::spidermonkeyruntimewrapper::SM_RT.with(move |sm_rc: &RefCell<SmRuntime>| {
                let sm_rt = &mut *sm_rc.borrow_mut();
                sm_rt.opt_es_rt_inner = Some(sm_ref_inner);
            });
        });

        // init default methods and es code

        features::init(&rt);
        es_sys_scripts::init_es(&rt);

        rt
    }

    /// start a thread which calls the cleanup method and then the garbage collector
    pub fn start_gc_deamon(&self, interval: Duration) {
        let wrc = Arc::downgrade(&self.inner);
        thread::spawn(move || loop {
            thread::sleep(interval);

            let arc_opt = wrc.upgrade();
            if let Some(arc) = arc_opt {
                arc.cleanup_sync();
            } else {
                // arc to inner dropped, stop deamon loop
                log::trace!("stopping esruntimewrapper clanup deamon loop...");
                break;
            }
        });
    }

    /// this method should be called when you want to run the garbage collector
    pub fn cleanup_sync(&self) {
        self.do_with_inner(move |inner| {
            inner.cleanup_sync();
        })
    }

    /// register a rust_op so it can be invoked from script by calling
    /// esses.invoke_rust_op(name, param1, param2) -> returns a Promise
    /// or
    /// esses.invoke_rust_op_sync(name, param1, param2) -> returns the result
    /// or
    /// esses.invoke_rust_op_void(name, param1, param2) -> returns nothing, but should be slightly faster then ignoring the promise from invoke_rust_op
    pub fn register_op(&self, name: &'static str, op: crate::spidermonkeyruntimewrapper::OP) {
        self.do_with_inner(|inner| {
            inner.register_op(name, op);
        });
    }

    /// eval a script and wait for it to complete
    pub fn eval_sync(&self, code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_inner(move |inner| inner.eval_sync(code, file_name))
    }

    pub fn load_module_sync(
        &self,
        module_src: &str,
        module_file_name: &str,
    ) -> Result<(), EsErrorInfo> {
        self.do_with_inner(|inner| inner.load_module_sync(module_src, module_file_name))
    }

    /// eval a script and wait for it to complete
    pub fn eval_void_sync(&self, code: &str, file_name: &str) -> Result<(), EsErrorInfo> {
        self.do_with_inner(move |inner| inner.eval_void_sync(code, file_name))
    }

    /// call a function by name and wait for it to complete
    pub fn call_sync(
        &self,
        obj_names: Vec<&'static str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_inner(move |inner| inner.call_sync(obj_names, function_name, args))
    }

    /// eval a script and don't wait for it to complete
    pub fn eval(&self, eval_code: &str, file_name: &str) {
        self.do_with_inner(move |inner| inner.eval(eval_code, file_name))
    }

    /// call a function by name and don't wait for it to complete
    pub fn call(
        &self,
        obj_names: Vec<&'static str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) {
        self.do_with_inner(move |inner| inner.call(obj_names, function_name, args))
    }

    pub fn do_with_inner<R, F: FnOnce(&EsRuntimeWrapperInner) -> R>(&self, f: F) -> R {
        let inner = self.inner.clone();
        f(&*inner)
    }

    pub fn do_in_es_runtime_thread<R: Send + 'static, J>(&self, immutable_job: J)
    where
        J: FnOnce(&SmRuntime) -> () + Send + 'static,
    {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread(immutable_job))
    }

    pub fn do_in_es_runtime_thread_sync<R: Send + 'static, J>(&self, immutable_job: J) -> R
    where
        J: FnOnce(&SmRuntime) -> R + Send + 'static,
    {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread_sync(immutable_job))
    }

    pub fn do_in_es_runtime_thread_mut_sync<R: Send + 'static, J>(&self, mutable_job: J) -> R
    where
        J: FnOnce(&mut SmRuntime) -> R + Send + 'static,
    {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread_mut_sync(mutable_job))
    }

    pub fn add_helper_task<T>(task: T)
    where
        T: FnOnce() -> () + Send + 'static,
    {
        let tm = HELPER_TASKS.clone();

        tm.add_task(task);
    }

    /// add a global function to the runtime which is callable just like any other js function
    ///
    /// # Example
    /// ```rust
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    /// fn test_add_global_sync_function(){
    ///     let rt = EsRuntimeWrapperBuilder::new().build();
    ///     rt.add_global_sync_function("test_add_global_sync", |_args| {
    ///          Ok(EsValueFacade::new_i32(361))
    ///     });
    ///     let esvf = rt.eval_sync("test_add_global_sync();", "test_add_global_sync_function.es").ok().expect("test_add_global_sync_function failed");
    ///     assert_eq!(esvf.get_i32(), &361);
    /// }
    /// ```
    pub fn add_global_sync_function<F>(&self, name: &'static str, func: F)
    where
        F: Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + 'static,
    {
        self.do_with_inner(move |inner| {
            inner.add_global_sync_function(name, func);
        })
    }

    /// add a global function to the runtime which is callable just like any other js function
    /// this async variant will run the method in a separate thread and return the result as a Promise
    /// # Example
    /// ```rust
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    /// use std::time::Duration;
    /// fn test_add_global_sync_function(){
    ///     let rt = EsRuntimeWrapperBuilder::new().build();
    ///     rt.add_global_async_function("test_add_global_async", |_args| {
    ///          Ok(EsValueFacade::new_i32(351))
    ///     });
    ///     let esvf = rt.eval_sync("test_add_global_async();", "test_add_global_async_function.es").ok().expect("test_add_global_async_function failed");
    ///     assert!(esvf.is_promise());
    ///     let prom_res = esvf.get_promise_result_blocking(Duration::from_secs(5)).ok().expect("promise timed out");
    ///     assert_eq!(prom_res.ok().expect("test_add_global_async_function failed").get_i32(), &351);
    /// }
    /// ```
    pub fn add_global_async_function<F>(&self, name: &'static str, func: F)
    where
        F: Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + Sync + 'static,
    {
        self.do_with_inner(move |inner| {
            inner.add_global_async_function(name, func);
        })
    }
}

#[cfg(test)]
pub mod tests {

    use crate::es_utils::EsErrorInfo;
    use crate::esruntimewrapper::EsRuntimeWrapper;
    use crate::esvaluefacade::EsValueFacade;
    use log::LevelFilter;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    lazy_static! {
        pub static ref TEST_RT: Arc<EsRuntimeWrapper> = init_test_runtime();
    }

    fn init_test_runtime() -> Arc<EsRuntimeWrapper> {
        log::info!("test: init_test_runtime");
        simple_logging::log_to_file("esruntimewrapper.log", LevelFilter::Trace)
            .ok()
            .unwrap();

        let module_code_loader = |file_name: &str| {
            format!("export default () => 123; export const other = Math.sqrt(8); console.log('running imported test module'); \n\nconsole.log('parsing a module from code loader for filename: {}');", file_name)
        };
        let rt = EsRuntimeWrapper::builder()
            .gc_interval(Duration::from_secs(2))
            .module_code_loader(Box::new(module_code_loader))
            .build();

        rt.do_in_es_runtime_thread_sync(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, _cx, _global| {
                // uncomment this to test with gc in sadistic mode
                // crate::es_utils::set_gc_zeal_options(_cx);
            })
        });

        Arc::new(rt)
    }

    #[test]
    fn test_gc() {
        log::info!("test: test_gc");
        simple_logging::log_to_file("esruntimewrapper.log", LevelFilter::Trace)
            .ok()
            .unwrap();

        let rt = EsRuntimeWrapper::builder()
            .gc_interval(Duration::from_secs(1))
            .build();

        rt.do_in_es_runtime_thread_sync(|sm_rt| {
            sm_rt
                .eval("this.f = () => {return 123;};", " test.es")
                .ok()
                .unwrap();

            let id = sm_rt.do_with_jsapi(|_rt, cx: *mut mozjs::jsapi::JSContext, _global| {
                let new_obj = crate::es_utils::promises::new_promise(cx);

                crate::spidermonkeyruntimewrapper::register_cached_object(cx, new_obj)
            });

            sm_rt.cleanup();

            sm_rt.do_with_jsapi(|_rt, cx: *mut mozjs::jsapi::JSContext, _global| {
                let p = crate::spidermonkeyruntimewrapper::consume_cached_object(id);
                let p_obj = p.get();
                rooted!(in (cx) let p_root = p_obj);
                rooted!(in (cx) let mut rval = mozjs::jsval::UndefinedValue());
                crate::es_utils::objects::get_es_obj_prop_val(
                    cx,
                    p_root.handle(),
                    "then",
                    rval.handle_mut(),
                )
                .ok()
                .unwrap();
            });

            let ret = sm_rt.call(vec![], "f", vec![]).ok().unwrap();
            println!("got {}", ret.get_i32());

            sm_rt.eval("1+1;", " test.es").ok().unwrap();
            sm_rt.cleanup();
        });
        std::thread::sleep(Duration::from_secs(3));
    }

    #[test]
    fn test_module() {
        log::info!("test: test_module");
        let esrt: Arc<EsRuntimeWrapper> = TEST_RT.clone();

        let load_mod_res = esrt.load_module_sync("import {other} from 'foo_test_mod.mes';\n\nlet test_method_0 = (a) => {return a * 11;};\n\nesses.test_method_1 = (a) => {return a * 12;};", "test_module_rt.mes");

        if load_mod_res.is_err() {
            let err = load_mod_res.err().unwrap();
            panic!(
                "error test_module: {}:{}:{} -> {}",
                err.filename, err.lineno, err.column, err.message
            );
        }

        thread::sleep(Duration::from_secs(4));

        let flubber_res = esrt.eval_sync(
            "let flubber = esses.test_method_1(5); flubber;",
            "test_module_2.es",
        );

        if flubber_res.is_err() {
            let err = flubber_res.err().unwrap();
            panic!(
                "error test_module: {}:{}:{} -> {}",
                err.filename, err.lineno, err.column, err.message
            );
        }

        let esvf = flubber_res.ok().unwrap();

        assert_eq!(esvf.get_i32(), &60);
    }

    #[test]
    fn call_method() {
        log::info!("test: call_method");
        let rt: Arc<EsRuntimeWrapper> = TEST_RT.clone();
        rt.eval_sync(
            "this.myObj = {childObj: {myMethod: function(a, b){return a*b;}}};",
            "call_method",
        )
        .ok()
        .unwrap();
        let call_res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(
            vec!["myObj", "childObj"],
            "myMethod",
            vec![EsValueFacade::new_i32(12), EsValueFacade::new_i32(14)],
        );
        match call_res {
            Ok(esvf) => println!("answer was {}", esvf.get_i32()),
            Err(eei) => println!("failed because {}", eei.message),
        }
    }

    #[test]
    fn test_async_await() {
        log::info!("test: test_async_await");
        let rt: Arc<EsRuntimeWrapper> = TEST_RT.clone();

        let code = "\
                    let async_method = async function(){\
                    let p = new Promise((resolve, reject) => {\
                    setImmediate(() => {\
                    resolve(123);\
                    });\
                    });\
                    return p;\
                    };\
                    \
                    let async_method_2 = async function(){\
                    let res = await async_method();\
                    return res;\
                    }; \
                    async_method_2();\
                    ";

        let prom_facade = rt.eval_sync(code, "call_method").ok().unwrap();
        let wait_res = prom_facade.get_promise_result_blocking(Duration::from_secs(60));
        let prom_res = wait_res.ok().unwrap();
        let esvf_res = prom_res.ok().unwrap();
        assert_eq!(&123, esvf_res.get_i32());
    }
}
