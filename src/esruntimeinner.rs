use crate::esruntime::ModuleCodeLoader;
use crate::esvaluefacade::EsValueFacade;
use crate::jsapi_utils::handles::from_raw_handle_mut;
use crate::jsapi_utils::{report_exception2, EsErrorInfo};
use crate::spidermonkeyruntimewrapper::SmRuntime;
use hirofa_utils::single_threaded_event_queue::SingleThreadedEventQueue;
use log::{debug, trace};
use mozjs::jsapi::CallArgs;
use std::sync::Arc;

pub struct EsRuntimeInner {
    pub(crate) event_queue: Arc<SingleThreadedEventQueue>,
    pub(crate) _pre_cleanup_tasks: Vec<Box<dyn Fn(&EsRuntimeInner) + Send + Sync>>,
    pub(crate) module_source_loader: Option<Box<ModuleCodeLoader>>,
    pub(crate) module_cache_size: usize,
}

impl EsRuntimeInner {
    pub(crate) fn build(
        module_source_loader: Option<Box<ModuleCodeLoader>>,
        module_cache_size: usize,
    ) -> Self {
        EsRuntimeInner {
            event_queue: SingleThreadedEventQueue::new(),
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

        self.do_in_es_event_queue(Box::new(move |sm_rt: &SmRuntime| {
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
        self.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.call(obj_names, f_n.as_str(), args)
        }))
    }

    pub fn eval(&self, eval_code: &str, file_name: &str) {
        debug!("eval {} in thread {}", eval_code, thread_id::get());

        let eval_code = eval_code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_event_queue(Box::new(move |sm_rt: &SmRuntime| {
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

        self.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.eval(eval_code.as_str(), file_name.as_str())
        }))
    }

    pub fn eval_void_sync(&self, code: &str, file_name: &str) -> Result<(), EsErrorInfo> {
        let eval_code = code.to_string();
        let file_name = file_name.to_string();

        self.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
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

        self.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.load_module(module_src_str.as_str(), module_file_name_str.as_str())
        }))
    }

    pub(crate) fn cleanup_sync(&self) {
        trace!("cleaning up es_rt");
        // todo, set is_cleaning var on inner, here and now
        // that should hint the engine to not use this runtime
        self.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
            sm_rt.cleanup();
        }));
        // reset cleaning var here
    }

    pub fn do_in_es_event_queue<J>(&self, job: J)
    where
        J: FnOnce(&SmRuntime) + Send + 'static,
    {
        trace!("do_in_es_runtime_thread");
        // this is executed in the single thread in the Threadpool, therefore Runtime and global are stored in a thread_local

        let async_job = || {
            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt| {
                debug!("got rt from thread_local");
                job(&mut sm_rt.borrow())
            })
        };

        self.event_queue.add_task(async_job);
    }

    pub fn do_in_es_event_queue_sync<R: Send + 'static, J>(&self, job: J) -> R
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

        self.event_queue.exe_task(job)
    }

    pub fn add_global_async_function<F>(&self, name: &'static str, func: F)
    where
        F: Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + Sync + 'static,
    {
        let func_rc = Arc::new(func);
        self.do_in_es_event_queue_sync(move |sm_rt| {
            sm_rt.add_global_function(name, move |cx, args: CallArgs| {
                let mut args_vec = vec![];

                for x in 0..args.argc_ {
                    let arg = args.get(x); // jsapi handle
                    let var_arg: mozjs::rust::HandleValue =
                        unsafe { mozjs::rust::Handle::from_raw(arg) };
                    args_vec.push(EsValueFacade::new_v(cx, var_arg));
                }

                let func_rc_clone = func_rc.clone();
                let prom_res_esvf = EsValueFacade::new_promise(move || func_rc_clone(args_vec));
                let rval = from_raw_handle_mut(args.rval());
                prom_res_esvf.to_es_value(cx, rval);
                true
            });
        });
    }

    pub fn add_global_sync_function<F>(&self, name: &'static str, func: F)
    where
        F: Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + 'static,
    {
        self.do_in_es_event_queue_sync(move |sm_rt| {
            sm_rt.add_global_function(name, move |cx, args: CallArgs| {
                let mut args_vec = vec![];

                for x in 0..args.argc_ {
                    let arg = args.get(x); // jsapi handle
                    let var_arg: mozjs::rust::HandleValue =
                        unsafe { mozjs::rust::Handle::from_raw(arg) };
                    args_vec.push(EsValueFacade::new_v(cx, var_arg));
                }

                let func_res = func(args_vec);
                match func_res {
                    Ok(esvf) => {
                        // set rval
                        let rval = from_raw_handle_mut(args.rval());
                        esvf.to_es_value(cx, rval);
                        true
                    }
                    Err(js_err) => {
                        // report es err
                        let s = format!("method failed\ncaused by: {}\0", js_err);
                        report_exception2(cx, s);
                        false
                    }
                }
            });
        });
    }
}

impl Drop for EsRuntimeInner {
    fn drop(&mut self) {
        self.do_in_es_event_queue_sync(Box::new(|_sm_rt: &SmRuntime| {
            debug!("dropping EsRuntimeWrapperInner");
        }));
    }
}
