use crate::es_utils::objects::get_es_obj_prop_val;
use crate::es_utils::{get_type_of, report_es_ex, EsErrorInfo};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFunction;
use mozjs::jsapi::JSNative;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSType;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_NewFunction;
use mozjs::jsapi::JS_ObjectIsFunction;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::JSVal;
use mozjs::jsval::UndefinedValue;
use mozjs::rust::jsapi_wrapped::JS_CallFunctionName;
use mozjs::rust::{HandleObject, MutableHandle};

/// call a method by name
pub fn call_method_name(
    context: *mut JSContext,
    scope: HandleObject,
    function_name: &str,
    args: Vec<JSVal>,
    ret_val: &mut MutableHandle<JSVal>,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    call_method_name2(
        context,
        scope,
        function_name,
        arguments_value_array,
        ret_val,
    )
}

/// call a method by name with a rooted arguments array
pub fn call_method_name2(
    context: *mut JSContext,
    scope: HandleObject,
    function_name: &str,
    args: HandleValueArray,
    ret_val: &mut MutableHandle<JSVal>,
) -> Result<(), EsErrorInfo> {
    let n = format!("{}\0", function_name);

    if unsafe {
        JS_CallFunctionName(
            context,
            scope.into(),
            n.as_ptr() as *const libc::c_char,
            &args,
            ret_val,
        )
    } {
        Ok(())
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

/// call a method by name on an object by name
/// e.g. esses.cleanup() can be called by calling
/// call_obj_method_name(cx, glob, vec!["esses"], "cleanup", vec![]);
#[allow(dead_code)]
pub fn call_obj_method_name(
    context: *mut JSContext,
    scope: HandleObject,
    obj_names: Vec<&str>,
    function_name: &str,
    args: Vec<JSVal>,
    ret_val: &mut MutableHandle<JSVal>,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    let mut sub_scope: *mut JSObject = *scope;
    for obj_name in obj_names {
        rooted!(in(context) let sub_scope_root = sub_scope);
        rooted!(in(context) let mut new_subscope_root = UndefinedValue());
        let res = get_es_obj_prop_val(
            context,
            sub_scope_root.handle(),
            obj_name,
            new_subscope_root.handle_mut(),
        );

        if res.is_err() {
            panic!(
                "could not get prop {}: {}",
                obj_name,
                res.err().unwrap().message
            );
        }

        let val: JSVal = *new_subscope_root.handle();

        if !val.is_object() {
            return Err(EsErrorInfo {
                message: format!("{} was not an object.", obj_name),
                column: 0,
                lineno: 0,
                filename: "".to_string(),
            });
        }

        sub_scope = val.to_object();
    }

    rooted!(in(context) let sub_scope_root = sub_scope);

    call_method_name2(
        context,
        sub_scope_root.handle(),
        function_name,
        arguments_value_array,
        ret_val,
    )
}

/// check whether an Value is a function
pub fn value_is_function(context: *mut JSContext, val: JSVal) -> bool {
    let js_type = get_type_of(context, val);
    js_type == JSType::JSTYPE_FUNCTION
}

/// check whether an object is a function
pub fn object_is_function(cx: *mut JSContext, obj: *mut JSObject) -> bool {
    unsafe { JS_ObjectIsFunction(cx, obj) }
}

/// define a new native function on an object
/// JSNative = Option<unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool>
pub fn define_native_function(
    cx: *mut JSContext,
    obj: HandleObject,
    function_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", function_name);

    let ret: *mut JSFunction = unsafe {
        JS_DefineFunction(
            cx,
            obj.into(),
            n.as_ptr() as *const libc::c_char,
            native_function,
            1,
            0,
        )
    };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

/// define a new native function
/// JSNative = Option<unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool>
pub fn new_native_function(
    cx: *mut JSContext,
    function_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", function_name);

    let ret: *mut JSFunction =
        unsafe { JS_NewFunction(cx, native_function, 1, 0, n.as_ptr() as *const libc::c_char) };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

#[cfg(test)]
mod tests {
    use crate::es_utils;
    use crate::es_utils::functions::{call_method_name, call_obj_method_name, value_is_function};
    use crate::es_utils::report_es_ex;
    use crate::es_utils::tests::test_with_sm_rt;
    use mozjs::jsval::{Int32Value, JSVal, UndefinedValue};

    #[test]
    fn test_instance_of_function() {
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                println!("evalling new func");
                let res = es_utils::eval(
                    rt,
                    global,
                    "(function test_func(){});",
                    "test_instance_of_function.es",
                    rval.handle_mut(),
                );
                if !res.is_ok() {
                    if let Some(err) = report_es_ex(cx) {
                        println!("err: {}", err.message);
                    }
                } else {
                    println!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    println!("getting obj {}", p_value.is_object());
                    return value_is_function(cx, p_value);
                }
                false
            })
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_method_by_name() {
        let ret = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());

                let _res = es_utils::eval(
                    rt,
                    global,
                    "this.test_method_by_name_func = function(a, b){return a * b;};",
                    "test_method_by_name.es",
                    rval.handle_mut(),
                );

                let a: JSVal = Int32Value(7);
                let b: JSVal = Int32Value(5);
                let fres = call_method_name(
                    cx,
                    global,
                    "test_method_by_name_func",
                    vec![a, b],
                    &mut rval.handle_mut(),
                );
                if fres.is_err() {
                    panic!(fres.err().unwrap().message);
                }
                let ret_val: JSVal = *rval;
                let ret: i32 = ret_val.to_int32();
                ret
            })
        });

        assert_eq!(ret, 35);
    }

    #[test]
    fn test_obj_method_by_name() {
        let ret = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {

                rooted!(in(cx) let mut rval = UndefinedValue());

                let res = es_utils::eval(
                    rt,
                    global,
                    "this.test_obj_method_by_name = {test_obj_method_by_name_func :function(a, b){return a * b;}};", "test_method_by_name.es",
                    rval.handle_mut()
                );

                if res.is_err() {
                    let err_res = report_es_ex(cx);
                    if let Some(err) = err_res {
                        println!("err {}", err.message);
                    }
                }

                let a: JSVal = Int32Value(7);
                let b: JSVal = Int32Value(5);
                let fres = call_obj_method_name(
                    cx,
                    global,
                    vec!["test_obj_method_by_name"],
                    "test_obj_method_by_name_func",
                    vec![a, b],
                    &mut rval.handle_mut(),
                );
                if fres.is_err() {
                    panic!(fres.err().unwrap().message);
                }
                let ret_val: JSVal = *rval;
                let ret: i32 = ret_val.to_int32();
                ret
            })
        });

        assert_eq!(ret, 35);
    }
}
