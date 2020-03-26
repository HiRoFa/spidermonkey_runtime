use crate::esvaluefacade::EsValueFacade;
use log::{debug, trace};
use mozjs::conversions::jsstr_to_string;
use mozjs::glue::RUST_JSID_IS_STRING;
use mozjs::glue::RUST_JSID_TO_STRING;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSString;
use mozjs::jsapi::JS_ClearPendingException;
use mozjs::jsapi::JS_GetConstructor;
use mozjs::jsapi::JS_GetObjectPrototype;
use mozjs::jsapi::JS_GetPendingException;
use mozjs::jsapi::JS_GetProperty;
use mozjs::jsapi::JS_IsExceptionPending;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_NewObjectWithGivenProto;
use mozjs::jsapi::JS_NewPlainObject;
use mozjs::jsapi::JS_NewStringCopyN;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsapi::JSCLASS_FOREGROUND_FINALIZE;
use mozjs::jsapi::JSITER_OWNONLY;
use mozjs::jsapi::JS_GC;
use mozjs::jsval::{JSVal, StringValue, UndefinedValue};
use mozjs::rust::jsapi_wrapped::GetPropertyKeys;
use mozjs::rust::jsapi_wrapped::JS_CallFunctionName;
use mozjs::rust::wrappers::JS_DefineProperty;
use mozjs::rust::{HandleObject, HandleValue, IdVector, IntoHandle, Runtime};
use std::marker::PhantomData;

use std::str;

/// get a single member of a JSObject
#[allow(dead_code)]
pub fn get_es_obj_prop_val(
    context: *mut JSContext,
    obj: *mut mozjs::jsapi::JSObject,
    prop_name: &str,
) -> mozjs::jsapi::Value {
    rooted!(in(context) let mut prop_val = UndefinedValue());

    let jso_handle: mozjs::jsapi::Handle<*mut mozjs::jsapi::JSObject> = mozjs::jsapi::Handle {
        ptr: &obj,
        _phantom_0: PhantomData,
    };
    let n = format!("{}\0", prop_name);
    unsafe {
        JS_GetProperty(
            context,
            jso_handle,
            n.as_ptr() as *const libc::c_char,
            prop_val.handle_mut().into(),
        );
    }
    *prop_val
}

/// see if there is a pending exception and return it
#[allow(dead_code)]
pub fn report_es_ex(context: *mut JSContext) -> Option<EsErrorInfo> {
    trace!("report_es_ex");

    let bln_ex: bool = unsafe { JS_IsExceptionPending(context) };
    let ret: Option<EsErrorInfo>;

    if bln_ex {
        rooted!(in(context) let mut error_value = UndefinedValue());
        if unsafe { JS_GetPendingException(context, error_value.handle_mut().into()) } {
            let js_error_obj: *mut mozjs::jsapi::JSObject = error_value.to_object();
            let message_value: mozjs::jsapi::Value =
                get_es_obj_prop_val(context, js_error_obj, "message");
            let filename_value: mozjs::jsapi::Value =
                get_es_obj_prop_val(context, js_error_obj, "fileName");
            let lineno_value: mozjs::jsapi::Value =
                get_es_obj_prop_val(context, js_error_obj, "lineNumber");
            let column_value: mozjs::jsapi::Value =
                get_es_obj_prop_val(context, js_error_obj, "columnNumber");

            let message_esvf = EsValueFacade::new_v(context, message_value);
            let filename_esvf = EsValueFacade::new_v(context, filename_value);
            let lineno_esvf = EsValueFacade::new_v(context, lineno_value);
            let column_esvf = EsValueFacade::new_v(context, column_value);

            let error_info: EsErrorInfo = EsErrorInfo {
                message: message_esvf.get_string().to_string(),
                filename: filename_esvf.get_string().to_string(),
                lineno: lineno_esvf.get_i32().clone(),
                column: column_esvf.get_i32().clone(),
            };

            debug!(
                "ex = {} in {} at {}:{}",
                error_info.message, error_info.filename, error_info.lineno, error_info.column
            );

            ret = Some(error_info);

            unsafe { JS_ClearPendingException(context) };
        } else {
            ret = None;
        }
    } else {
        ret = None;
    }

    ret
}

/// struct that represents a script exception
pub struct EsErrorInfo {
    pub message: String,
    pub filename: String,
    pub lineno: i32,
    pub column: i32,
}

impl Clone for EsErrorInfo {
    fn clone(&self) -> Self {
        EsErrorInfo {
            message: self.message.clone(),
            filename: self.filename.clone(),
            lineno: self.lineno.clone(),
            column: self.column.clone(),
        }
    }
}

/// eval a piece of source code in the engine
pub fn eval(
    runtime: &Runtime,
    scope: *mut JSObject,
    code: &str,
    file_name: &str,
) -> Result<EsValueFacade, EsErrorInfo> {
    let context = runtime.cx();

    rooted!(in(context) let scope_root = scope);
    let scope = scope_root.handle();

    rooted!(in(context) let mut rval = UndefinedValue());

    let eval_res = runtime.evaluate_script(scope, code, file_name, 0, rval.handle_mut());

    if eval_res.is_ok() {
        Ok(EsValueFacade::new(context, rval.handle()))
    } else {
        let ex_opt = report_es_ex(context);
        if let Some(ex) = ex_opt {
            Err(ex)
        } else {
            Err(EsErrorInfo {
                message: "unknown error while evalling".to_string(),
                filename: file_name.to_string(),
                lineno: 0,
                column: 0,
            })
        }
    }
}

/// call a method by name
#[allow(dead_code)]
pub fn call_method_name(
    context: *mut JSContext,
    scope: *mut JSObject,
    function_name: &str,
    args: Vec<EsValueFacade>,
) -> Result<EsValueFacade, EsErrorInfo> {
    let n = format!("{}\0", function_name);
    rooted!(in(context) let mut rval = UndefinedValue());

    rooted!(in(context) let scope_root = scope);
    let scope_handle = scope_root.handle();

    let mut arguments_value_vec: Vec<JSVal> = vec![];

    for arg_vf in args {
        arguments_value_vec.push(arg_vf.to_es_value(context));
    }

    let arguments_value_array =
        unsafe { HandleValueArray::from_rooted_slice(&*arguments_value_vec) };

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
        Ok(EsValueFacade::new(context, rval.handle()))
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

/// create a new object in the engine
#[allow(dead_code)]
pub fn new_object(context: *mut JSContext) -> *mut JSObject {
    unsafe { JS_NewPlainObject(context) }
}

/// create a new object based on a prototype object
#[allow(dead_code)]
pub fn new_object_from_prototype(
    context: *mut JSContext,
    prototype: HandleObject,
) -> Result<*mut JSObject, EsErrorInfo> {
    let ret: *mut JSObject =
        unsafe { JS_NewObjectWithGivenProto(context, &CLASS as *const _, prototype.into_handle()) };
    if ret.is_null() {
        let err_opt = report_es_ex(context);
        if err_opt.is_some() {
            Err(err_opt.unwrap())
        } else {
            Ok(ret)
        }
    } else {
        Ok(ret)
    }
}

static CLASS_OPS: JSClassOps = JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: None,
    mayResolve: None,
    finalize: None,
    call: None,
    hasInstance: None,
    construct: None,
    trace: None,
};

static CLASS: JSClass = JSClass {
    name: b"SimpleClass\0" as *const u8 as *const libc::c_char,
    flags: JSCLASS_FOREGROUND_FINALIZE,
    cOps: &CLASS_OPS as *const JSClassOps,
    reserved: [0 as *mut _; 3],
};

/// get the prototype of an object
#[allow(dead_code)]
pub fn get_prototype(
    context: *mut JSContext,
    obj: HandleObject,
) -> Result<*mut JSObject, EsErrorInfo> {
    let ret: *mut JSObject = unsafe { JS_GetObjectPrototype(context, obj.into_handle()) };

    if ret.is_null() {
        let err_opt = report_es_ex(context);
        if err_opt.is_some() {
            Err(err_opt.unwrap())
        } else {
            Ok(ret)
        }
    } else {
        Ok(ret)
    }
}

/// get the constructor of an object
#[allow(dead_code)]
pub fn get_constructor(
    context: *mut JSContext,
    obj: HandleObject,
) -> Result<*mut JSObject, EsErrorInfo> {
    let ret: *mut JSObject = unsafe { JS_GetConstructor(context, obj.into_handle()) };

    if ret.is_null() {
        let err_opt = report_es_ex(context);
        if err_opt.is_some() {
            Err(err_opt.unwrap())
        } else {
            Ok(ret)
        }
    } else {
        Ok(ret)
    }
}

/// set a property of an object
#[allow(dead_code)]
pub fn set_es_obj_prop_val(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
    prop_val: HandleValue,
) {
    let prop_name_str = format!("{}\0", prop_name);
    unsafe {
        JS_DefineProperty(
            context,
            obj,
            prop_name_str.as_ptr() as *const libc::c_char,
            prop_val,
            mozjs::jsapi::JSPROP_ENUMERATE as u32,
        );
    }
}

/// convert a string to a StringValue so it can be used in the engine
#[allow(dead_code)]
pub fn new_es_value_from_str(context: *mut JSContext, s: &str) -> mozjs::jsapi::Value {
    let js_string: *mut JSString =
        unsafe { JS_NewStringCopyN(context, s.as_ptr() as *const libc::c_char, s.len()) };
    //mozjs::jsapi::JS_NewStringCopyZ(context, s.as_ptr() as *const libc::c_char);
    return StringValue(unsafe { &*js_string });
}

/// convert a StringValue to a rust string
#[allow(dead_code)]
pub fn es_value_to_str(context: *mut JSContext, val: &mozjs::jsapi::Value) -> String {
    let jsa: *mut mozjs::jsapi::JSString = val.to_string();
    return es_jsstring_to_string(context, jsa);
}

/// convert a JSString to a rust string
pub fn es_jsstring_to_string(
    context: *mut JSContext,
    js_string: *mut mozjs::jsapi::JSString,
) -> String {
    unsafe {
        return jsstr_to_string(context, js_string);
    }
}

/// call the garbage collector
pub fn gc(context: *mut JSContext) {
    unsafe {
        JS_GC(context);
    }
}

/// get all the propertynames of an object
#[allow(dead_code)]
pub fn get_js_obj_prop_names(context: *mut JSContext, obj: *mut JSObject) -> Vec<String> {
    rooted!(in(context) let obj_root = obj);
    let obj_handle = obj_root.handle();

    let ids = unsafe { IdVector::new(context) };

    assert!(unsafe { GetPropertyKeys(context, obj_handle.into(), JSITER_OWNONLY, ids.get()) });

    let mut ret: Vec<String> = vec![];

    for x in 0..ids.len() {
        rooted!(in(context) let id = ids[x]);

        assert!(unsafe { RUST_JSID_IS_STRING(id.handle().into()) });
        rooted!(in(context) let id = unsafe{RUST_JSID_TO_STRING(id.handle().into())});

        //let id_esvf = EsValueFacade::new_v(context, id.handle());

        let prop_name = es_jsstring_to_string(context, *id);

        ret.push(prop_name);
    }
    ret
}

#[cfg(test)]
mod tests {
    use crate::es_utils::{
        call_method_name, es_value_to_str, eval, get_es_obj_prop_val, get_js_obj_prop_names,
        report_es_ex, EsErrorInfo,
    };

    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::UndefinedValue;
    use std::collections::HashMap;

    #[test]
    fn test_get_js_obj_prop_names() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
                let context = runtime.cx();

                rooted!(in(context) let global_root = sm_rt.global_obj);
                let global = global_root.handle();

                rooted!(in(context) let mut rval = UndefinedValue());
                let _eval_res = runtime.evaluate_script(
                    global,
                    "({a: 1, b: 2, c: 3})",
                    "test_get_js_obj_prop_names.es",
                    0,
                    rval.handle_mut(),
                );
                let e_opt = report_es_ex(context);
                assert!(e_opt.is_none());

                let jso = rval.to_object();

                get_js_obj_prop_names(context, jso)
            }))
        });

        assert_eq!(test_vec.len(), 3);
        assert_eq!(test_vec.get(0).unwrap(), &"a".to_string());
        assert_eq!(test_vec.get(1).unwrap(), &"b".to_string());
        assert_eq!(test_vec.get(2).unwrap(), &"c".to_string());
    }

    #[test]
    fn test_get_js_obj_prop_values() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
                let context = runtime.cx();

                rooted!(in(context) let global_root = sm_rt.global_obj);
                let global = global_root.handle();

                rooted!(in(context) let mut rval = UndefinedValue());
                let _eval_res = runtime.evaluate_script(
                    global,
                    "({a: '1', b: '2', c: '3'})",
                    "test_get_js_obj_prop_values.es",
                    0,
                    rval.handle_mut(),
                );
                let e_opt = report_es_ex(context);
                assert!(e_opt.is_none());

                let jso = rval.to_object();

                let prop_vec: Vec<String> = get_js_obj_prop_names(context, jso);

                let mut test_vec = vec![];
                for prop_name in prop_vec {
                    let prop_val = get_es_obj_prop_val(context, jso, prop_name.as_str());
                    test_vec.push(es_value_to_str(context, &prop_val).to_string());
                }

                test_vec
            }))
        });

        assert_eq!(test_vec.len(), 3);
        assert_eq!(test_vec.get(0).unwrap(), &"1".to_string());
        assert_eq!(test_vec.get(1).unwrap(), &"2".to_string());
        assert_eq!(test_vec.get(2).unwrap(), &"3".to_string());
    }

    #[test]
    fn test_es_value_to_string() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_string: String = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
                let context = runtime.cx();

                rooted!(in(context) let global_root = sm_rt.global_obj);
                let global = global_root.handle();

                rooted!(in(context) let mut rval = UndefinedValue());
                let _eval_res = runtime.evaluate_script(
                    global,
                    "('this is a string')",
                    "test_es_value_to_string.es",
                    0,
                    rval.handle_mut(),
                );
                let e_opt = report_es_ex(context);
                assert!(e_opt.is_none());

                es_value_to_str(context, &rval).to_string()
            }))
        });

        assert_eq!(test_string, "this is a string".to_string());
    }

    #[test]
    fn test_a_lot() {
        for _x in 0..20 {
            test_call_method_name();
            test_get_obj_props();
            test_get_js_obj_prop_names();
            test_get_js_obj_prop_values();
            test_es_value_to_string();
        }
    }

    #[test]
    fn test_get_obj_props() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.eval_sync("return {a: 1, b: 'abc', c: true, d: 'much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string'};", "test_get_obj_props");
        assert!(res.is_ok());
        let map = res.ok().unwrap();
        let map: &HashMap<String, EsValueFacade> = map.get_object();
        assert_eq!(map.get(&"b".to_string()).unwrap().get_string(), "abc");
    }

    #[test]
    fn test_call_method_name() {
        //simple_logger::init().unwrap();

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(

                Box::new(|sm_rt: &SmRuntime| {
                    let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
                    let context = runtime.cx();

                    rooted!(in(context) let global_root = sm_rt.global_obj);
                    let global = global_root.handle();

                    rooted!(in(context) let mut rval = UndefinedValue());
                    let _eval_res = runtime.evaluate_script(
                        global,
                        "this.test_func_1 = function test_func_1(a, b, c){return (a + '_' + b + '_' + c);};",
                        "test_call_method_name.es",
                        0,
                        rval.handle_mut(),
                    );

                    let esvf: EsValueFacade = call_method_name(
                        context,
                        sm_rt.global_obj,
                        "test_func_1",
                        vec![
                            EsValueFacade::new_str("abc".to_string()),
                            EsValueFacade::new_bool(true),
                            EsValueFacade::new_i32(123)
                        ],
                    ).ok().unwrap();

                    esvf.get_string().to_string()
                }
            ))
        });

        assert_eq!(res, "abc_true_123".to_string());
    }

    #[test]
    fn test_eval() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res: String = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;

                let res: Result<EsValueFacade, EsErrorInfo> = eval(
                    runtime,
                    sm_rt.global_obj,
                    "let a = 'i am eval'; a",
                    "test_eval.es",
                );

                res.ok().unwrap().get_string().to_string()
            }))
        });

        assert_eq!(res, "i am eval".to_string());
    }

    #[test]
    fn test_report_exception() {
        //simple_logger::init().unwrap();

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
                let context = runtime.cx();

                rooted!(in(context) let global_root = sm_rt.global_obj);
                let global = global_root.handle();

                report_es_ex(context);

                rooted!(in(context) let mut rval = UndefinedValue());
                let _ = runtime.evaluate_script(
                    global,
                    "let b = quibus * 12;",
                    "test_ex.es",
                    0,
                    rval.handle_mut(),
                );

                let ex_opt = report_es_ex(context);
                if let Some(ex) = ex_opt {
                    return ex.message;
                }

                "".to_string()
            }))
        });

        assert_eq!(res, "quibus is not defined");
    }
}
