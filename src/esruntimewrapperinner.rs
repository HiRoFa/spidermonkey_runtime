use crate::es_utils::EsErrorInfo;
use crate::esruntimewrapper::ModuleCodeLoader;
use crate::esvaluefacade::EsValueFacade;
use crate::microtaskmanager::MicroTaskManager;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use log::{debug, trace};
use std::sync::Arc;

pub struct EsRuntimeWrapperInner {
    pub(crate) task_manager: Arc<MicroTaskManager>,
    pub(crate) _pre_cleanup_tasks: Vec<Box<dyn Fn(&EsRuntimeWrapperInner) -> () + Send + Sync>>,
    pub(crate) module_source_loader: Option<Box<dyn Fn(&str) -> String + Send + Sync>>,
    pub(crate) module_cache_size: usize,
}

impl EsRuntimeWrapperInner {
    pub(crate) fn build(
        module_source_loader: Option<Box<ModuleCodeLoader>>,
        module_cache_size: usize,
    ) -> Self {
        EsRuntimeWrapperInner {
            task_manager: MicroTaskManager::new(),
            _pre_cleanup_tasks: vec![],
            module_source_loader,
            module_cache_size,
        }
    }

    pub fn call(
        &self,
        obj_names: Vec<&'static str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) {
        debug!("call {} in thread {}", function_name, thread_id::get());
        let f_n = function_name.to_string();

        self.do_in_es_runtime_thread(Box::new(move |sm_rt: &SmRuntime| {
            let res = sm_rt.call(obj_names, f_n.as_str(), args);
            if res.is_err() {
                debug!("async call failed: {}", res.err().unwrap().message);
            }
        }))
    }

    pub fn call_sync(
        &self,
        obj_names: Vec<&'static str>,
        function_name: &str,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("call_sync {} in thread {}", function_name, thread_id::get());
        let f_n = function_name.to_string();
        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.call(obj_names, f_n.as_str(), args)
        }))
    }

    pub fn eval(&self, eval_code: &str, file_name: &str) {
        debug!("eval {} in thread {}", eval_code, thread_id::get());

        let eval_code = eval_code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_runtime_thread(Box::new(move |sm_rt: &SmRuntime| {
            let res = sm_rt.eval_void(eval_code.as_str(), file_name.as_str());
            if res.is_err() {
                debug!("async code eval failed: {}", res.err().unwrap().message);
            }
        }))
    }

    pub fn eval_sync(&self, code: &str, file_name: &str) -> Result<EsValueFacade, EsErrorInfo> {
        debug!("eval_sync {} in thread {}", code, thread_id::get());
        let eval_code = code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.eval(eval_code.as_str(), file_name.as_str())
        }))
    }

    pub fn eval_void_sync(&self, code: &str, file_name: &str) -> Result<(), EsErrorInfo> {
        let eval_code = code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.eval_void(eval_code.as_str(), file_name.as_str())
        }))
    }

    pub fn load_module_sync(
        &self,
        module_src: &str,
        module_file_name: &str,
    ) -> Result<(), EsErrorInfo> {
        let module_src_str = module_src.to_string();
        let module_file_name_str = module_file_name.to_string();

        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.load_module(module_src_str.as_str(), module_file_name_str.as_str())
        }))
    }

    pub(crate) fn cleanup_sync(&self) {
        trace!("cleaning up es_rt");
        // todo, set is_cleaning var on inner, here and now
        // that should hint the engine to not use this runtime
        self.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.cleanup();
        }));
        // reset cleaning var here
    }

    pub fn do_in_es_runtime_thread<J>(&self, job: J)
    where
        J: FnOnce(&SmRuntime) -> () + Send + 'static,
    {
        trace!("do_in_es_runtime_thread");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let async_job = || {
            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                job(&mut sm_rt.borrow())
            })
        };

        self.task_manager.add_task(async_job);
    }

    pub fn do_in_es_runtime_thread_sync<R: Send + 'static, J>(&self, job: J) -> R
    where
        J: FnOnce(&SmRuntime) -> R + Send + 'static,
    {
        trace!("do_in_es_runtime_thread_sync");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let job = || {
            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                job(&mut sm_rt.borrow())
            })
        };

        self.task_manager.exe_task(job)
    }

    pub fn do_in_es_runtime_thread_mut_sync<R: Send + 'static, J>(&self, mutable_job: J) -> R
    where
        J: FnOnce(&mut SmRuntime) -> R + Send + 'static,
    {
        trace!("do_in_es_runtime_thread_mut_sync");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let job = || {
            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                mutable_job(&mut sm_rt.borrow_mut())
            })
        };

        self.task_manager.exe_task(job)
    }
    pub(crate) fn register_op(
        &self,
        name: &'static str,
        op: crate::spidermonkeyruntimewrapper::OP,
    ) {
        self.do_in_es_runtime_thread_mut_sync(Box::new(move |sm_rt: &mut SmRuntime| {
            sm_rt.register_op(name, op);
        }));
    }
}

impl Drop for EsRuntimeWrapperInner {
    fn drop(&mut self) {
        self.do_in_es_runtime_thread_mut_sync(Box::new(|_sm_rt: &mut SmRuntime| {
            debug!("dropping EsRuntimeWrapperInner");
        }));
    }
}
