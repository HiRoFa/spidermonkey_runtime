use mozjs::rust::jsapi_wrapped::NewPromiseObject;
use mozjs::rust::HandleObject;

use crate::es_utils::objects::{get_constructor, get_es_obj_prop_val_as_string};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::NullValue;

pub fn object_is_promise(context: *mut JSContext, obj: HandleObject) -> bool {
    // todo this is not the best way of doing this, we need to get the promise object of the global scope and see if that is the same as the objects constructor

    let constr_res = get_constructor(context, obj);
    if constr_res.is_ok() {
        let constr: *mut JSObject = constr_res.ok().unwrap();
        rooted!(in (context) let constr_root = constr);
        if !constr.is_null() {
            let name_prop = get_es_obj_prop_val_as_string(context, constr_root.handle(), "name");

            return name_prop.as_str().eq("Promise");
        }
    }
    false
}

/// create a new Promise, this will be used later by invoke_rust_op
pub fn new_promise(context: *mut JSContext) -> *mut JSObject {
    // second is executor
    // third is proto
    rooted!(in(context) let null = NullValue().to_object());
    let null_handle: HandleObject = null.handle();
    unsafe { NewPromiseObject(context, null_handle, null_handle) }
}

pub fn new_promise_with_exe(context: *mut JSContext, executor: HandleObject) -> *mut JSObject {
    rooted!(in(context) let null = NullValue().to_object());
    let null_handle: HandleObject = null.handle();
    unsafe { NewPromiseObject(context, executor, null_handle) }
}

#[cfg(test)]
mod tests {
    use crate::es_utils;
    use crate::es_utils::promises::object_is_promise;
    use crate::es_utils::report_es_ex;
    use crate::es_utils::tests::test_with_sm_rt;
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
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                trace!("evalling new promise obj");
                let res = es_utils::eval(
                    rt,
                    global,
                    "new Promise((res, rej) => {});",
                    "test_instance_of_promise.es",
                    rval.handle_mut(),
                );
                if !res.is_ok() {
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
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                trace!("evalling some obj");
                let res = es_utils::eval(
                    rt,
                    global,
                    "({some: 'obj'});",
                    "test_not_instance_of_promise.es",
                    rval.handle_mut(),
                );
                if !res.is_ok() {
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
}
