use crate::jsapi_utils::{get_pending_exception, EsErrorInfo};
use mozjs::jsapi::AddPromiseReactions;
use mozjs::jsapi::GetPromiseResult;
use mozjs::jsapi::GetPromiseState;
use mozjs::jsapi::HandleObject as RawHandleObject;
use mozjs::jsapi::IsPromiseObject;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::PromiseState;
use mozjs::jsapi::SetPromiseRejectionTrackerCallback;
use mozjs::jsapi::StackFormat;
use mozjs::jsval::{JSVal, NullValue};
use mozjs::rust::jsapi_wrapped::NewPromiseObject;
use mozjs::rust::jsapi_wrapped::RejectPromise;
use mozjs::rust::jsapi_wrapped::ResolvePromise;
use mozjs::rust::{HandleObject, HandleValue, MutableHandleValue};
use std::os::raw::c_void;
use std::ptr;

/// Returns true if the given object is an unwrapped PromiseObject, false otherwise.
pub fn object_is_promise(obj: HandleObject) -> bool {
    object_is_promise_raw(obj.into())
}

/// return true if the given JSVal is a Promise
pub fn value_is_promise(val: HandleValue) -> bool {
    if val.is_object() {
        let obj: *mut JSObject = val.to_object();
        let obj_handle = unsafe { HandleObject::from_marked_location(&obj) };
        object_is_promise(obj_handle)
    } else {
        false
    }
}

/// Returns true if the given object is an unwrapped PromiseObject, false otherwise.
pub fn object_is_promise_raw(obj: RawHandleObject) -> bool {
    unsafe { IsPromiseObject(obj) }
}

/// Returns the given Promise's result: either the resolution value for fulfilled promises, or the rejection reason for rejected ones.
pub fn get_promise_result(promise: HandleObject) -> JSVal {
    get_promise_result_raw(promise.into())
}

/// Returns the given Promise's result: either the resolution value for fulfilled promises, or the rejection reason for rejected ones.
pub fn get_promise_result_raw(promise: RawHandleObject) -> JSVal {
    unsafe { GetPromiseResult(promise) }
}

/// Unforgeable, optimized version of the JS builtin Promise.prototype.then.
/// Takes a Promise instance and onResolve, onReject callables to enqueue as reactions for that promise. In difference to Promise.prototype.then, this doesn't create and return a new Promise instance.
/// Throws a TypeError if promise isn't a Promise (or possibly a different error if it's a security wrapper or dead object proxy).
/// Asserts that onFulfilled and onRejected are each either callable or null.
pub fn add_promise_reactions(
    cx: *mut JSContext,
    promise: HandleObject,
    then: HandleObject,
    catch: HandleObject,
) -> bool {
    add_promise_reactions_raw(cx, promise.into(), then.into(), catch.into())
}

/// Unforgeable, optimized version of the JS builtin Promise.prototype.then.
/// Takes a Promise instance and onResolve, onReject callables to enqueue as reactions for that promise. In difference to Promise.prototype.then, this doesn't create and return a new Promise instance.
/// Throws a TypeError if promise isn't a Promise (or possibly a different error if it's a security wrapper or dead object proxy).
/// Asserts that onFulfilled and onRejected are each either callable or null.
pub fn add_promise_reactions_raw(
    cx: *mut JSContext,
    promise: RawHandleObject,
    then: RawHandleObject,
    catch: RawHandleObject,
) -> bool {
    unsafe { AddPromiseReactions(cx, promise, then, catch) }
}

/// convert two closures to JSObjects and add them to the promise as reactions
pub fn add_promise_reactions_callbacks<T, C>(
    cx: *mut JSContext,
    promise: HandleObject,
    then_opt: Option<T>,
    catch_opt: Option<C>,
) -> bool
where
    T: Fn(*mut JSContext, Vec<HandleValue>, MutableHandleValue) -> Result<(), String> + 'static,
    C: Fn(*mut JSContext, Vec<HandleValue>, MutableHandleValue) -> Result<(), String> + 'static,
{
    rooted!(in (cx) let mut then_rval = NullValue().to_object_or_null());
    rooted!(in (cx) let mut catch_rval = NullValue().to_object_or_null());

    if let Some(then) = then_opt {
        assert!(crate::jsapi_utils::functions::new_callback(
            cx,
            then_rval.handle_mut(),
            then
        ));
    }
    if let Some(catch) = catch_opt {
        assert!(crate::jsapi_utils::functions::new_callback(
            cx,
            catch_rval.handle_mut(),
            catch
        ));
    }

    add_promise_reactions(cx, promise, then_rval.handle(), catch_rval.handle())
}

/// Returns the given Promise's state as a JS::PromiseState enum value.
/// Returns JS::PromiseState::Pending if the given object is a wrapper that can't safely be unwrapped.
pub fn get_promise_state(promise: HandleObject) -> PromiseState {
    unsafe { GetPromiseState(promise.into()) }
}

/// Returns the given Promise's state as a JS::PromiseState enum value.
/// Returns JS::PromiseState::Pending if the given object is a wrapper that can't safely be unwrapped.
pub fn get_promise_state_raw(promise: RawHandleObject) -> PromiseState {
    unsafe { GetPromiseState(promise) }
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
    } else if let Some(err) = get_pending_exception(context) {
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
    } else if let Some(err) = get_pending_exception(context) {
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
    use crate::jsapi_utils::get_pending_exception;
    use crate::jsapi_utils::promises::object_is_promise;
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
                    if let Some(err) = get_pending_exception(cx) {
                        trace!("err: {}", err.message);
                    }
                } else {
                    trace!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    trace!("getting obj {}", p_value.is_object());

                    rooted!(in(cx) let prom_obj_root = p_value.to_object());
                    trace!("is_prom");
                    return object_is_promise(prom_obj_root.handle());
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
                    if let Some(err) = get_pending_exception(cx) {
                        trace!("err: {}", err.message);
                    }
                } else {
                    trace!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    trace!("getting obj {}", p_value.is_object());
                    rooted!(in(cx) let prom_obj_root = p_value.to_object());
                    trace!("is_prom");
                    return object_is_promise(prom_obj_root.handle());
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
