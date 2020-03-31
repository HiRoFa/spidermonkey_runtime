use log::{debug, trace};
use mozjs::conversions::jsstr_to_string;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSString;
use mozjs::jsapi::JS_ClearPendingException;
use mozjs::jsapi::JS_GetPendingException;
use mozjs::jsapi::JS_IsExceptionPending;
use mozjs::jsapi::JS_NewStringCopyN;
use mozjs::jsapi::JS_GC;
use mozjs::jsval::{JSVal, StringValue, UndefinedValue};
use mozjs::rust::Runtime;
use mozjs::jsapi::JSType;
use mozjs::jsapi::JS_TypeOfValue;

use std::str;
use crate::es_utils::objects::{get_es_obj_prop_val_as_string, get_es_obj_prop_val_as_i32};

pub mod promises;
pub mod functions;
pub mod arrays;
pub mod objects;



/// get the type of a JSVal
pub fn get_type_of(context: *mut JSContext, val: JSVal) -> JSType {
    rooted!(in(context) let val_root = val);
    unsafe {JS_TypeOfValue(context, val_root.handle().into())}
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
            rooted!(in(context) let mut js_error_obj_root = js_error_obj);

            let message=
                get_es_obj_prop_val_as_string(context, js_error_obj_root.handle(), "message");
            let filename =
                get_es_obj_prop_val_as_string(context, js_error_obj_root.handle(), "fileName");
            let lineno =
                get_es_obj_prop_val_as_i32(context, js_error_obj_root.handle(), "lineNumber");
            let column =
                get_es_obj_prop_val_as_i32(context, js_error_obj_root.handle(), "columnNumber");

            let error_info: EsErrorInfo = EsErrorInfo {
                message,
                filename,
                lineno,
                column,
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
) -> Result<JSVal, EsErrorInfo> {

    // todo rebuild this so you have to pass a mut handle which you need to root yourself before calling this

    let context = runtime.cx();

    rooted!(in(context) let scope_root = scope);
    let scope = scope_root.handle();

    rooted!(in(context) let mut rval = UndefinedValue());

    let eval_res = runtime.evaluate_script(scope, code, file_name, 0, rval.handle_mut());

    if eval_res.is_ok() {
        Ok(*rval)
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



#[cfg(test)]
mod tests {
    use crate::es_utils::{es_value_to_str, eval, report_es_ex, EsErrorInfo};

    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::{UndefinedValue, JSVal};



    pub fn test_with_sm_rt<F, R: Send + 'static>(test_fn: F) -> R where F: FnOnce(&SmRuntime) -> R + Send + 'static {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(test_fn))
        })
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
            test_es_value_to_string();
        }
    }


    #[test]
    fn test_eval() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res: String = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;

                let res: Result<JSVal, EsErrorInfo> = eval(
                    runtime,
                    sm_rt.global_obj,
                    "let a = 'i am eval'; a",
                    "test_eval.es",
                );
                let str = es_value_to_str(runtime.cx(), &res.ok().unwrap());
                str
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
