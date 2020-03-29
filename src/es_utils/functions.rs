
use mozjs::jsapi::JSType;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::JSVal;
use crate::es_utils::{get_type_of, EsErrorInfo, report_es_ex};
use mozjs::jsapi::JS_ObjectIsFunction;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::UndefinedValue;
use mozjs::rust::jsapi_wrapped::JS_CallFunctionName;
use mozjs::jsapi::JS_NewArrayObject;



/// call a method by name
#[allow(dead_code)]
pub fn call_method_name (
    context: *mut JSContext,
    scope: *mut JSObject,
    function_name: &str,
    args: Vec<JSVal>,
) -> Result<JSVal, EsErrorInfo> {

    // todo rebuild so you need to pass a mut handle which you need to root yourself before calling this

    let n = format!("{}\0", function_name);
    rooted!(in(context) let mut rval = UndefinedValue());

    rooted!(in(context) let scope_root = scope);
    let scope_handle = scope_root.handle();


    let arguments_value_array =
        unsafe { HandleValueArray::from_rooted_slice(&*args) };

    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    if unsafe {
        JS_CallFunctionName(
            context,
            scope_handle.into(),
            n.as_ptr() as *const libc::c_char,
            &arguments_value_array,
            &mut rval.handle_mut(),
        )
    } {
        Ok(*rval)
    } else {
        if let Some(err) = report_es_ex(context) {
            Err(err)
        } else {
            Err(EsErrorInfo {
                message: "unknown error".to_string(),
                filename: "".to_string(),
                lineno: 0,
                column: 0,
            })
        }
    }
}



/// check whether an Value is a function
pub fn value_is_function(context: *mut JSContext, val: JSVal) -> bool {
    let js_type = get_type_of(context, val);
    js_type == JSType::JSTYPE_FUNCTION
}

/// check whether an object is a function
pub fn object_is_function(cx: *mut JSContext, obj: *mut JSObject) -> bool {
    unsafe{JS_ObjectIsFunction(cx, obj)}
}

#[cfg(test)]
mod tests {
    use crate::es_utils::tests::test_with_sm_rt;
    use crate::es_utils::report_es_ex;
    use crate::es_utils::functions::{ value_is_function};
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_instance_of_function() {
        let res = test_with_sm_rt(|sm_rt| {
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();
            rooted!(in(context) let global_root = global);
            rooted!(in(context) let mut rval = UndefinedValue());
            println!("evalling new func");
            let res = sm_rt.runtime.evaluate_script(global_root.handle(), "(function test_func(){});", "test_instance_of_function.es", 0, rval.handle_mut());
            if !res.is_ok() {
                if let Some(err) = report_es_ex(context) {
                    println!("err: {}", err.message);
                }
            } else {
                println!("getting value");
                let p_value: mozjs::jsapi::Value = *rval;
                println!("getting obj {}", p_value.is_object());
                return value_is_function(context, p_value);
            }
            false

        });
        assert_eq!(res, true);
    }
}