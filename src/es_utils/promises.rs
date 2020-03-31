use mozjs::rust::jsapi_wrapped::NewPromiseObject;
use mozjs::rust::HandleObject;

use crate::es_utils::objects::{get_constructor, get_es_obj_prop_val_as_string};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::NullValue;

pub fn object_is_promise(context: *mut JSContext, _scope: HandleObject, obj: HandleObject) -> bool {
    // todo this is not the best way of doing this, we need to get the promise object of the global scope and see if that is the same as the objects constructor
    // that's why the function requires the global_scope obj

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
    use crate::es_utils::promises::object_is_promise;
    use crate::es_utils::report_es_ex;
    use crate::es_utils::tests::test_with_sm_rt;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_instance_of_promise() {
        let res = test_with_sm_rt(|sm_rt| {
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();
            rooted!(in(context) let global_root = global);
            rooted!(in(context) let mut rval = UndefinedValue());
            println!("evalling new promise obj");
            let res = sm_rt.runtime.evaluate_script(
                global_root.handle(),
                "new Promise((res, rej) => {});",
                "test_instance_of_promise.es",
                0,
                rval.handle_mut(),
            );
            if !res.is_ok() {
                if let Some(err) = report_es_ex(context) {
                    println!("err: {}", err.message);
                }
            } else {
                println!("getting value");
                let p_value: mozjs::jsapi::Value = *rval;
                println!("getting obj {}", p_value.is_object());

                rooted!(in(context) let prom_obj_root = p_value.to_object());
                println!("is_prom");
                return object_is_promise(context, global_root.handle(), prom_obj_root.handle());
            }
            false
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_not_instance_of_promise() {
        let res = test_with_sm_rt(|sm_rt| {
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();
            rooted!(in(context) let global_root = global);
            rooted!(in(context) let mut rval = UndefinedValue());
            println!("evalling some obj");
            let res = sm_rt.runtime.evaluate_script(global_root.handle(), "let some_obj_test_not_instance_of_promise = {some: 'obj'}; some_obj_test_not_instance_of_promise;", "test_not_instance_of_promise.es", 0, rval.handle_mut());
            if !res.is_ok() {
                if let Some(err) = report_es_ex(context) {
                    println!("err: {}", err.message);
                }
            } else {
                println!("getting value");
                let p_value: mozjs::jsapi::Value = *rval;
                println!("getting obj {}", p_value.is_object());
                rooted!(in(context) let prom_obj_root = p_value.to_object());
                println!("is_prom");
                return object_is_promise(context, global_root.handle(), prom_obj_root.handle());
            }
            false
        });
        assert_eq!(res, false);
    }
}
