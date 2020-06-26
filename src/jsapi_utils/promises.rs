use crate::jsapi_utils::objects::{get_constructor, get_es_obj_prop_val_as_string};
use crate::jsapi_utils::{report_es_ex, EsErrorInfo};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::SetPromiseRejectionTrackerCallback;
use mozjs::jsapi::StackFormat;
use mozjs::jsval::NullValue;
use mozjs::rust::jsapi_wrapped::NewPromiseObject;
use mozjs::rust::jsapi_wrapped::RejectPromise;
use mozjs::rust::jsapi_wrapped::ResolvePromise;
use mozjs::rust::{HandleObject, HandleValue};
use std::os::raw::c_void;
use std::ptr;

/// see if a JSObject is an instance of Promise
pub fn object_is_promise(context: *mut JSContext, obj: HandleObject) -> bool {
    // todo this is not the best way of doing this, we need to get the promise object of the global scope and see if that is the same as the objects constructor

    let constr_res = get_constructor(context, obj);
    if constr_res.is_ok() {
        let constr: *mut JSObject = constr_res.ok().unwrap();
        rooted!(in (context) let constr_root = constr);
        if !constr.is_null() {
            let name_prop = get_es_obj_prop_val_as_string(context, constr_root.handle(), "name");
            if let Ok(name) = name_prop {
                return name.as_str().eq("Promise");
            }
        }
    }
    false
}

/// create a new Promise, this can be resolved later from rust
pub fn new_promise(context: *mut JSContext) -> *mut JSObject {
    // second is executor

    rooted!(in(context) let null = NullValue().to_object_or_null());
    let null_handle: HandleObject = null.handle();
    unsafe { NewPromiseObject(context, null_handle) }
}

/// create a new Promise, this will run the executor function with 2 args (resolve, reject)
/// this is the rust equivalent of the script
/// ```javascript
/// new promise(function(resolve, reject){});
/// ```
pub fn new_promise_with_exe(context: *mut JSContext, executor: HandleObject) -> *mut JSObject {
    unsafe { NewPromiseObject(context, executor) }
}

/// resolve a Promise with a given resolution value
pub fn resolve_promise(
    context: *mut JSContext,
    promise: HandleObject,
    resolution_value: HandleValue,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { ResolvePromise(context, promise, resolution_value) };
    if ok {
        Ok(())
    } else if let Some(err) = report_es_ex(context) {
        Err(err)
    } else {
        Err(EsErrorInfo {
            message: "unknown error resolving promise".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        })
    }
}

/// resolve a Promise with a given rejection value
pub fn reject_promise(
    context: *mut JSContext,
    promise: HandleObject,
    rejection_value: HandleValue,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { RejectPromise(context, promise, rejection_value) };
    if ok {
        Ok(())
    } else if let Some(err) = report_es_ex(context) {
        Err(err)
    } else {
        Err(EsErrorInfo {
            message: "unknown error rejecting promise".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::jsapi_utils;
    use crate::jsapi_utils::promises::object_is_promise;
    use crate::jsapi_utils::report_es_ex;
    use crate::jsapi_utils::tests::test_with_sm_rt;
    use log::trace;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_x() {
        for _x in 0..10 {
            test_instance_of_promise();
            test_not_instance_of_promise();
        }
    }

    #[test]
    fn test_instance_of_promise() {
        log::info!("test: test_instance_of_promise");
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                trace!("evalling new promise obj");
                let res = jsapi_utils::eval(
                    rt,
                    global,
                    "new Promise((res, rej) => {});",
                    "test_instance_of_promise.es",
                    rval.handle_mut(),
                );
                if res.is_err() {
                    if let Some(err) = report_es_ex(cx) {
                        trace!("err: {}", err.message);
                    }
                } else {
                    trace!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    trace!("getting obj {}", p_value.is_object());

                    rooted!(in(cx) let prom_obj_root = p_value.to_object());
                    trace!("is_prom");
                    return object_is_promise(cx, prom_obj_root.handle());
                }
                false
            })
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_not_instance_of_promise() {
        log::info!("test: test_not_instance_of_promise");
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                trace!("evalling some obj");
                let res = jsapi_utils::eval(
                    rt,
                    global,
                    "({some: 'obj'});",
                    "test_not_instance_of_promise.es",
                    rval.handle_mut(),
                );
                if res.is_err() {
                    if let Some(err) = report_es_ex(cx) {
                        trace!("err: {}", err.message);
                    }
                } else {
                    trace!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    trace!("getting obj {}", p_value.is_object());
                    rooted!(in(cx) let prom_obj_root = p_value.to_object());
                    trace!("is_prom");
                    return object_is_promise(cx, prom_obj_root.handle());
                }
                false
            })
        });
        assert_eq!(res, false);
    }

    #[test]
    fn test_promise_rejection_log() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.eval_sync(
            "{let p = new Promise((res, rej) => {rej('poof');}); p.then((res) => {});}",
            "test_promise_rejection_log.es",
        )
        .ok()
        .expect("script test_promise_rejection_log.es failed");
    }
}

/// this initializes a default rejectiontracker which logs when a promise was rejected which did not have a rejection handler
pub fn init_rejection_tracker(cx: *mut JSContext) {
    unsafe {
        SetPromiseRejectionTrackerCallback(cx, Some(promise_rejection_tracker), ptr::null_mut())
    };
}

unsafe extern "C" fn promise_rejection_tracker(
    cx: *mut JSContext,
    _muted_errors: bool,
    _promise: mozjs::jsapi::HandleObject,
    _state: mozjs::jsapi::PromiseRejectionHandlingState,
    _data: *mut c_void,
) {
    capture_stack!(in (cx) let stack);
    let str_stack = stack
        .unwrap()
        .as_string(None, StackFormat::SpiderMonkey)
        .unwrap();

    log::error!(
        "promise without rejection handler was rejected from:\n{}",
        str_stack
    );
}
