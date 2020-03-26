use log::trace;

use std::{str, thread};

use std::sync::{Arc, Weak};

use crate::es_sys;
use crate::features;

use crate::es_utils::EsErrorInfo;
use crate::esruntimewrapperinner::{EsRuntimeWrapperInner, ImmutableJob, MutableJob};
use crate::esvaluefacade::EsValueFacade;
use crate::microtaskmanager::MicroTaskManager;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use crate::taskmanager::TaskManager;
use std::cell::RefCell;
use std::time::Duration;

lazy_static! {
    /// a static Multithread taskmanager used to run rust ops async and multithreaded
    static ref HELPER_TASKS: Arc<TaskManager> = Arc::new(TaskManager::new(num_cpus::get()));
}

/// the EsRuntimeWrapper is a facade that adds all script todos to the SmRuntimeWrapper's MicroTaskManager so they are invoked in a single worker thread
/// you can wait for those tasks to complete by calling the _sync variants of the public methods here
pub struct EsRuntimeWrapper {
    inner: Arc<EsRuntimeWrapperInner>,
}

impl EsRuntimeWrapper {
    pub fn new(_pre_cleanup_tasks: Option<Vec<Box<dyn Fn(&EsRuntimeWrapperInner) -> ()>>>) -> Self {
        let inner = EsRuntimeWrapperInner {
            task_manager: MicroTaskManager::new(),
        };
        let arc_inner = Arc::new(inner);
        let sm_ref_inner: Weak<EsRuntimeWrapperInner> = Arc::downgrade(&arc_inner);
        let rt = EsRuntimeWrapper { inner: arc_inner };

        // pass arc around inner to sm_rt thread

        rt.inner.task_manager.exe_task(move || {
            crate::spidermonkeyruntimewrapper::SM_RT.with(move |sm_rc: &RefCell<SmRuntime>| {
                let sm_rt = &mut *sm_rc.borrow_mut();
                sm_rt.opt_es_rt_inner = Some(sm_ref_inner);
            });
        });

        // init default methods and es code

        features::init(&rt);
        es_sys::init_es(&rt);

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

    /// call a function by name and wait for it to complete
    pub fn call_sync(
        &self,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        self.do_with_inner(move |inner| inner.call_sync(function_name, args))
    }

    /// eval a script and don't wait for it to complete
    pub fn eval(&self, eval_code: &str, file_name: &str) -> () {
        self.do_with_inner(move |inner| inner.eval(eval_code, file_name))
    }

    /// call a function by name and don't wait for it to complete
    pub fn call(&self, function_name: &str, args: Vec<EsValueFacade>) -> () {
        self.do_with_inner(move |inner| inner.call(function_name, args))
    }

    pub fn do_with_inner<R, F: FnOnce(&EsRuntimeWrapperInner) -> R>(&self, f: F) -> R {
        trace!("about to lock inner");
        let inner = self.inner.clone();
        trace!("got lock on inner");
        f(&*inner)
    }

    pub fn do_in_es_runtime_thread(&self, immutable_job: ImmutableJob<()>) -> () {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread(immutable_job))
    }

    pub fn do_in_es_runtime_thread_sync<R: Send + 'static>(
        &self,
        immutable_job: ImmutableJob<R>,
    ) -> R {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread_sync(immutable_job))
    }

    pub fn do_in_es_runtime_thread_mut_sync(&self, mutable_job: MutableJob<()>) -> () {
        self.do_with_inner(|inner| inner.do_in_es_runtime_thread_mut_sync(mutable_job))
    }

    pub fn add_helper_task<T>(task: T)
    where
        T: FnOnce() -> () + Send + 'static,
    {
        let tm = HELPER_TASKS.clone();

        tm.add_task(task);
    }
}

#[cfg(test)]
pub mod tests {
    use crate::es_utils::EsErrorInfo;
    use crate::esruntimewrapper::EsRuntimeWrapper;
    use crate::esvaluefacade::EsValueFacade;
    use log::debug;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    lazy_static! {
        pub static ref TEST_RT: Arc<EsRuntimeWrapper> = Arc::new(EsRuntimeWrapper::new(None));
    }

    #[test]
    fn test() {
        simple_logger::init().unwrap();
        let esrt: Arc<EsRuntimeWrapper> = TEST_RT.clone();
        esrt.start_gc_deamon(Duration::from_secs(1));
        thread::sleep(Duration::from_secs(6));
        debug!("should have cleaned up at least 5 times by now");
    }

    #[test]
    fn example() {
        // start a runtime

        let rt = EsRuntimeWrapper::new(None);

        // create an example object

        rt.eval_sync("this.myObj = {a: 1, b: 2};", "test1.es")
            .ok()
            .unwrap();

        // register a native rust method

        rt.register_op(
            "my_rusty_op",
            Box::new(|_sm_rt, args: Vec<EsValueFacade>| {
                let a = args.get(0).unwrap().get_i32();
                let b = args.get(1).unwrap().get_i32();
                Ok(EsValueFacade::new_i32(a * b))
            }),
        );

        // call the rust method from ES

        rt.eval_sync(
            "this.myObj.c = esses.invoke_rust_op_sync('my_rusty_op', 3, 7);",
            "test2.es",
        )
        .ok()
        .unwrap();

        let c: Result<EsValueFacade, EsErrorInfo> =
            rt.eval_sync("return(this.myObj.c);", "test3.es");

        assert_eq!(&21, c.ok().unwrap().get_i32());

        // define an ES method and calling it from rust

        rt.eval_sync("this.my_method = (a, b) => {return a * b;};", "test4.es")
            .ok()
            .unwrap();

        let args = vec![EsValueFacade::new_i32(12), EsValueFacade::new_i32(5)];
        let c_res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync("my_method", args);
        let c: EsValueFacade = c_res.ok().unwrap();
        assert_eq!(&60, c.get_i32());
    }
}
