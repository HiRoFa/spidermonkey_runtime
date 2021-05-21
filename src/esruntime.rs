use std::{str, thread};

use std::sync::{Arc, Weak};

use crate::es_sys_scripts;
use crate::features;

use crate::esruntimeinner::EsRuntimeInner;
use crate::esvaluefacade::EsValueFacade;
use crate::jsapi_utils::EsErrorInfo;

use crate::esruntimebuilder::EsRuntimeBuilder;
use crate::spidermonkeyruntimewrapper::SmRuntime;

use std::cell::RefCell;
use std::time::Duration;

use hirofa_utils::js_utils::Script;
use hirofa_utils::task_manager::TaskManager;

lazy_static! {
    /// a static Multithreaded taskmanager used to run rust ops async and multithreaded ( in at least 2 threads)
    static ref HELPER_TASKS: Arc<TaskManager> = Arc::new(TaskManager::new(std::cmp::max(2, num_cpus::get())));
}

/// the EsRuntime is a facade that adds all script todo's to the EsRuntimes's event queue so they are invoked in a single worker thread
/// you can wait for those tasks to complete by calling the _sync variants of the public methods here
pub struct EsRuntime {
    inner: Arc<EsRuntimeInner>,
}

/// A ModuleCodeLoader function is used to load code into the runtime
/// The first argument is the (relative) path of the module to import
/// The second argument is the absolute path to the module which is importing the new module (reference_path)
/// the EsScriptCode struct which is returned should allways contain an absolute path even if the module is loaded with a relative path
pub type ModuleCodeLoader = dyn Fn(&str, &str) -> Option<Script> + Send + Sync + 'static;

impl EsRuntime {
    /// create a builder to instantiate an EsRuntime
    pub fn builder() -> EsRuntimeBuilder {
        EsRuntimeBuilder::new()
    }

    pub(crate) fn new_inner(inner: EsRuntimeInner) -> Self {
        let arc_inner = Arc::new(inner);
        let sm_ref_inner: Weak<EsRuntimeInner> = Arc::downgrade(&arc_inner);
        let rt = EsRuntime { inner: arc_inner };

        // pass arc around inner to sm_rt thread

        rt.inner.event_loop.exe(move || {
            // todo this should also be in init_info

            crate::spidermonkeyruntimewrapper::SM_RT.with(move |sm_rc: &RefCell<SmRuntime>| {
                let sm_rt = &mut *sm_rc.borrow_mut();
                sm_rt.opt_esrt_inner = Some(sm_ref_inner);
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
                // arc to inner dropped, stop demon loop
                log::trace!("stopping EsRuntime cleanup demon loop...");
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

    /// eval a script and wait for it to complete
    pub fn eval_sync(&self, code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_inner(move |inner| inner.eval_sync(code, file_name))
    }

    /// load a script module and run it
    /// # Example
    /// ```rust
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.load_module_sync("console.log('running a module, you can import and export in and from modules');", "test_module.mes");
    /// ```
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
    /// # Example
    /// ```rust
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.eval_sync("this.com = {stuff: {method: function(){console.log('my func');}}}", "test_call_sync.es").ok().expect("script failed");
    /// rt.call_sync(vec!["com", "stuff"], "method", vec![]).ok().expect("call method failed");
    /// ```
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

    pub fn do_with_inner<R, F: FnOnce(&EsRuntimeInner) -> R>(&self, f: F) -> R {
        let inner = self.inner.clone();
        f(&*inner)
    }

    /// run a closure in the worker thread of this runtime's event queue, this is needed
    /// if you want to use the inner SmRuntime on which u can use the jsapi_utils
    pub fn do_in_es_event_queue<J>(&self, immutable_job: J)
    where
        J: FnOnce(&SmRuntime) + Send + 'static,
    {
        self.do_with_inner(|inner| inner.do_in_es_event_queue(immutable_job))
    }

    /// run a closure in the worker thread of this runtime's event queue and wait for it to complete,
    /// this is needed if you want to use the inner SmRuntime on which u can use the jsapi_utils
    pub fn do_in_es_event_queue_sync<R: Send + 'static, J>(&self, immutable_job: J) -> R
    where
        J: FnOnce(&SmRuntime) -> R + Send + 'static,
    {
        self.do_with_inner(|inner| inner.do_in_es_event_queue_sync(immutable_job))
    }

    /// add a task the the "helper" thread pool
    pub fn add_helper_task<T>(task: T)
    where
        T: FnOnce() + Send + 'static,
    {
        log::trace!("adding a helper task");

        let tm = HELPER_TASKS.clone();

        tm.add_task(task);
    }

    /// add a global function to the runtime which is callable just like any other js function
    ///
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.add_global_sync_function("test_add_global_sync", |_args| {
    ///      Ok(EsValueFacade::new_i32(361))
    /// });
    /// let esvf = rt.eval_sync("test_add_global_sync();", "test_add_global_sync_function.es").ok().expect("test_add_global_sync_function failed");
    /// assert_eq!(esvf.get_i32(), 361);
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
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    /// use std::time::Duration;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.add_global_async_function("test_add_global_async", |_args| {
    ///     Ok(EsValueFacade::new_i32(351))
    /// });
    /// let esvf = rt.eval_sync("test_add_global_async();", "test_add_global_async_function.es").ok().expect("test_add_global_async_function failed");
    /// assert!(esvf.is_promise());
    /// let prom_res = esvf.get_promise_result_blocking(Duration::from_secs(5)).ok().expect("promise timed out");
    /// assert_eq!(prom_res.ok().expect("test_add_global_async_function failed").get_i32(), 351);
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

    use crate::esruntime::EsRuntime;
    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils::EsErrorInfo;
    use hirofa_utils::js_utils::Script;
    use log::LevelFilter;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    lazy_static! {
        pub static ref TEST_RT: Arc<EsRuntime> = init_test_runtime();
    }

    fn init_test_runtime() -> Arc<EsRuntime> {
        log::info!("test: init_test_runtime");
        simple_logging::log_to_file("esruntime.log", LevelFilter::Trace)
            .ok()
            .unwrap();

        let module_code_loader = |path: &str, _ref_path: &str| {
            if path.eq("dontfind.mes") {
                None
            } else if path.eq("buggy.mes") {
                let code = format!(
                    "i'm a little teapot short and stout, Tip me over and pour me out!, {}",
                    path
                );
                Some(Script::new(path, code.as_str()))
            } else {
                let code = format!("export default () => 123; export const other = Math.sqrt(8); console.log('running imported test module'); \n\nconsole.log('parsing a module from code loader for filename: {}');", path);
                Some(Script::new(path, code.as_str()))
            }
        };
        let rt = EsRuntime::builder()
            .gc_interval(Duration::from_secs(2))
            .module_code_loader(Box::new(module_code_loader))
            .build();

        rt.do_in_es_event_queue_sync(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, _cx, _global| {
                // uncomment this to test with gc in sadistic mode
                crate::jsapi_utils::set_gc_zeal_options(_cx);
            })
        });

        Arc::new(rt)
    }

    #[test]
    fn test_gc() {
        log::info!("test: test_gc");
        simple_logging::log_to_file("esruntime.log", LevelFilter::Trace)
            .ok()
            .unwrap();

        let rt = EsRuntime::builder()
            .gc_interval(Duration::from_secs(1))
            .build();

        rt.do_in_es_event_queue_sync(|sm_rt| {
            sm_rt
                .eval("this.f = () => {return 123;};", " test.es")
                .ok()
                .unwrap();

            let id = sm_rt.do_with_jsapi(|_rt, cx: *mut mozjs::jsapi::JSContext, _global| {
                let new_obj = crate::jsapi_utils::promises::new_promise(cx);

                crate::spidermonkeyruntimewrapper::register_cached_object(cx, new_obj)
            });

            sm_rt.cleanup();

            sm_rt.do_with_jsapi(|_rt, cx: *mut mozjs::jsapi::JSContext, _global| {
                let p = crate::spidermonkeyruntimewrapper::remove_cached_object(id);
                let p_obj = p.get();
                rooted!(in (cx) let p_root = p_obj);
                rooted!(in (cx) let mut rval = mozjs::jsval::UndefinedValue());
                crate::jsapi_utils::objects::get_es_obj_prop_val(
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
    fn test_wasm() {
        let esrt: Arc<EsRuntime> = TEST_RT.clone();
        let esvf = esrt
            .eval_sync("typeof WebAssembly;", "test_wasm.es")
            .ok()
            .expect("script failed");
        assert!(esvf.is_string());
        assert_eq!(esvf.get_string(), "object");
    }

    #[test]
    fn test_module() {
        log::info!("test: test_module");
        let esrt: Arc<EsRuntime> = TEST_RT.clone();

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

        assert_eq!(esvf.get_i32(), 60);
    }

    #[test]
    fn call_method_2() {
        call_method();
        call_method();
    }

    #[test]
    fn call_method() {
        log::debug!("test: call_method");
        let rt: Arc<EsRuntime> = TEST_RT.clone();
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
        let rt: Arc<EsRuntime> = TEST_RT.clone();

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

        log::info!("test: test_async_await / 1");
        let prom_facade = rt.eval_sync(code, "call_method").ok().unwrap();
        log::info!("test: test_async_await / 2");
        let wait_res = prom_facade.get_promise_result_blocking(Duration::from_secs(60));
        log::info!("test: test_async_await / 3");
        let prom_res = wait_res.ok().unwrap();
        log::info!("test: test_async_await / 4");
        let esvf_res = prom_res.ok().unwrap();
        log::info!("test: test_async_await / 5");
        assert_eq!(123, esvf_res.get_i32());
        log::info!("test: test_async_await / 6");
    }
}
