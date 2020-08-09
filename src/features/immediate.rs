use crate::esruntime::EsRuntime;
use crate::jsapi_utils::report_exception;
use log::{error, trace};
use mozjs::jsval::ObjectValue;

pub(crate) fn init(rt: &EsRuntime) {
    rt.do_in_es_event_queue_sync(|sm_rt| {
        sm_rt.add_global_function("setImmediate", |cx, args| {
            if args.argc_ == 0 {
                report_exception(cx, "setImmediate requires at least one argument");
                return false;
            }

            if args.argc_ > 1 {
                report_exception(cx, "setImmediate does not support arguments for now");
                return false;
            }

            let func_val_handle = args.get(0);
            let func_val = *func_val_handle;
            let is_func = crate::jsapi_utils::functions::value_is_function(cx, func_val);
            if !is_func {
                report_exception(cx, "setImmediate requires a function as its first argument");
                return false;
            }

            // cache function
            let cached_id =
                crate::spidermonkeyruntimewrapper::register_cached_object(cx, func_val.to_object());

            // todo support args

            // invoke later
            let rt = crate::spidermonkeyruntimewrapper::SmRuntime::clone_current_esrt_inner_arc();
            rt.do_in_es_event_queue(move |sm_rt| {
                sm_rt.do_with_jsapi(|_rt, cx, global| {
                    let func_epr =
                        crate::spidermonkeyruntimewrapper::remove_cached_object(cached_id);
                    let func_obj = func_epr.get();

                    rooted!(in (cx) let mut rval = mozjs::jsval::UndefinedValue());
                    let val = ObjectValue(func_obj);
                    rooted!(in (cx) let mut val_root = val);

                    let res = crate::jsapi_utils::functions::call_function_value(
                        cx,
                        global,
                        val_root.handle(),
                        vec![],
                        rval.handle_mut(),
                    );
                    if res.is_err() {
                        error!(
                            "error executing setImmediate func: {}",
                            res.err().unwrap().err_msg()
                        );
                    } else {
                        trace!("executed setImmediate function");
                    }
                });
            });

            true
        });
    });
}

#[cfg(test)]
pub mod tests {
    use crate::esruntimebuilder::EsRuntimeBuilder;

    #[test]
    fn test_set_immediate() {
        let rt = EsRuntimeBuilder::new().build();
        let res = rt.eval_sync(
            "setImmediate(function(){console.log('logging immediate');});",
            "test_set_immediate.es",
        );
        if res.is_err() {
            panic!(
                "could not eval setImmediate: {}",
                res.err().unwrap().err_msg()
            );
        }
        assert!(res.is_ok())
    }
}
