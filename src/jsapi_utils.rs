//! # jsapi_utils
//!
//! this mod contains utilities for working with JSAPI(SpiderMonkey)
//! unless states otherwise you can asume that all methods in this mod and submods need to be called
//! from the event queue of the EsRuntime
//!
//! # Example
//!
//! ```no_run
//!     use es_runtime::esruntimebuilder::EsRuntimeBuilder;
//!     use es_runtime::jsapi_utils;
//!
//! let rt = EsRuntimeBuilder::new().build();
//! rt.do_in_es_event_queue_sync(|sm_rt| {
//!     // use jsapi_utils here
//!     // if you need the Runtime, JSContext or Global object (which you almost allways will)
//!     // you can use this method in the SmRuntime
//!     sm_rt.do_with_jsapi(|rt, cx, global| {
//!         // use jsapi_utils here
//!         let there_is_a_pending_exception = jsapi_utils::get_pending_exception(cx).is_some();
//!     })
//! })
//! ```
//!

#![allow(clippy::not_unsafe_ptr_arg_deref)]
use crate::jsapi_utils::objects::{get_es_obj_prop_val_as_i32, get_es_obj_prop_val_as_string};
use log::{debug, trace};
use mozjs::conversions::jsstr_to_string;
use mozjs::glue::{RUST_JSID_IS_STRING, RUST_JSID_TO_STRING};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSString;
use mozjs::jsapi::JSType;
use mozjs::jsapi::JS_ClearPendingException;
use mozjs::jsapi::JS_GetPendingException;
use mozjs::jsapi::JS_IsExceptionPending;
use mozjs::jsapi::JS_NewStringCopyN;
use mozjs::jsapi::JS_TypeOfValue;
use mozjs::jsapi::JS_GC;
use mozjs::jsval::{JSVal, StringValue, UndefinedValue};
use mozjs::rust::{HandleObject, MutableHandleValue, Runtime};
use std::str;

pub mod arrays;
pub mod functions;
pub mod handles;
pub mod modules;
pub mod objects;
pub mod promises;
pub mod reflection;
pub mod rooting;
pub mod scripts;

/// get the type of a JSVal
/// this is the equivalent of calling ```typeof val``` in script
pub fn get_type_of(context: *mut JSContext, val: JSVal) -> JSType {
    rooted!(in(context) let val_root = val);
    unsafe { JS_TypeOfValue(context, val_root.handle().into()) }
}

#[cfg(not(target = "release"))]
pub fn set_gc_zeal_options(cx: *mut JSContext) {
    use mozjs::jsapi::JS_SetGCZeal;
    debug!("setting gc_zeal_options");

    let level = 2;
    let frequency = 1; //JS_DEFAULT_ZEAL_FREQ;
    unsafe { JS_SetGCZeal(cx, level, frequency) };
}

#[cfg(target = "release")]
pub fn set_gc_zeal_options(_cx: *mut JSContext) {
    debug!("not setting gc_zeal_options");
}

pub fn report_exception(cx: *mut JSContext, ex: &str) {
    let ex_str = format!("{}\0", ex);
    unsafe {
        mozjs::jsapi::JS_ReportErrorUTF8(cx, ex_str.as_str().as_ptr() as *const libc::c_char)
    };
}

pub fn report_exception2(cx: *mut JSContext, ex: String) {
    let ex_str = format!("{}\0", ex);
    unsafe {
        mozjs::jsapi::JS_ReportErrorUTF8(cx, ex_str.as_str().as_ptr() as *const libc::c_char)
    };
}

/// see if there is a pending exception and return it as an EsErrorInfo
#[allow(dead_code)]
pub fn get_pending_exception(context: *mut JSContext) -> Option<EsErrorInfo> {
    trace!("report_es_ex");

    if unsafe { JS_IsExceptionPending(context) } {
        rooted!(in(context) let mut error_value = UndefinedValue());
        if unsafe { JS_GetPendingException(context, error_value.handle_mut().into()) } {
            let js_error_obj: *mut mozjs::jsapi::JSObject = error_value.to_object();
            rooted!(in(context) let mut js_error_obj_root = js_error_obj);

            let message =
                get_es_obj_prop_val_as_string(context, js_error_obj_root.handle(), "message")
                    .ok()
                    .unwrap();
            let filename =
                get_es_obj_prop_val_as_string(context, js_error_obj_root.handle(), "fileName")
                    .ok()
                    .unwrap();
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

            unsafe { JS_ClearPendingException(context) };
            Some(error_info)
        } else {
            None
        }
    } else {
        None
    }
}

/// struct that represents a script exception
pub struct EsErrorInfo {
    pub message: String,
    pub filename: String,
    pub lineno: i32,
    pub column: i32,
}

impl EsErrorInfo {
    /// get eror as String in the form of [message] at [filename]:[lineno]:[column]
    pub fn err_msg(&self) -> String {
        format!(
            "{} at {}:{}:{}",
            self.message, self.filename, self.lineno, self.column
        )
    }
}

impl Clone for EsErrorInfo {
    fn clone(&self) -> Self {
        EsErrorInfo {
            message: self.message.clone(),
            filename: self.filename.clone(),
            lineno: self.lineno,
            column: self.column,
        }
    }
}

/// eval a piece of source code in the engine
pub fn eval(
    runtime: &Runtime,
    scope: HandleObject,
    code: &str,
    file_name: &str,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let context = runtime.cx();

    let eval_res = runtime.evaluate_script(scope, code, file_name, 0, ret_val);

    if eval_res.is_ok() {
        Ok(())
    } else {
        let ex_opt = get_pending_exception(context);
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

/// convert a str to a StringValue so it can be used in the engine
// todo, refactor to accept rval #25
#[allow(dead_code)]
pub fn new_es_value_from_str(context: *mut JSContext, s: &str) -> mozjs::jsapi::Value {
    let js_string: *mut JSString =
        unsafe { JS_NewStringCopyN(context, s.as_ptr() as *const libc::c_char, s.len()) };
    //mozjs::jsapi::JS_NewStringCopyZ(context, s.as_ptr() as *const libc::c_char);
    StringValue(unsafe { &*js_string })
}

/// convert a StringValue to a rust string
#[allow(dead_code)]
pub fn es_value_to_str(
    context: *mut JSContext,
    val: mozjs::jsapi::Value,
) -> Result<String, &'static str> {
    if val.is_string() {
        let jsa: *mut mozjs::jsapi::JSString = val.to_string();
        Ok(es_jsstring_to_string(context, jsa))
    } else {
        Err("value was not a String")
    }
}

/// convert a JSString to a rust string
pub fn es_jsstring_to_string(
    context: *mut JSContext,
    js_string: *mut mozjs::jsapi::JSString,
) -> String {
    unsafe { jsstr_to_string(context, js_string) }
}

// convert a PropertyKey or JSID to String
pub fn es_jsid_to_string(context: *mut JSContext, id: mozjs::jsapi::HandleId) -> String {
    assert!(unsafe { RUST_JSID_IS_STRING(id) });
    rooted!(in(context) let id_str = unsafe{RUST_JSID_TO_STRING(id)});
    es_jsstring_to_string(context, *id_str)
}

/// call the garbage collector
pub fn gc(context: *mut JSContext) {
    unsafe {
        JS_GC(context, mozjs::jsapi::GCReason::API);
    }
}

#[cfg(test)]
mod tests {
    use crate::jsapi_utils::{es_value_to_str, get_pending_exception, EsErrorInfo};

    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::UndefinedValue;

    pub fn test_with_sm_rt<F, R: Send + 'static>(test_fn: F) -> R
    where
        F: FnOnce(&SmRuntime) -> R + Send + 'static,
    {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_in_es_event_queue_sync(test_fn)
    }

    #[test]
    fn test_es_value_to_string() {
        let rt = crate::esruntime::tests::TEST_RT.clone();

        let test_string: String = rt.do_with_inner(|inner| {
            inner.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    rooted!(in(cx) let mut rval = UndefinedValue());

                    let _eval_res = rt.evaluate_script(
                        global,
                        "('this is a string')",
                        "test_es_value_to_string.es",
                        0,
                        rval.handle_mut(),
                    );
                    let e_opt = get_pending_exception(cx);
                    assert!(e_opt.is_none());

                    es_value_to_str(cx, *rval).ok().unwrap()
                })
            })
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
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let res: String = rt.do_with_inner(|inner| {
            inner.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
                let res: Result<EsValueFacade, EsErrorInfo> =
                    sm_rt.eval("let a = 'i am eval'; a", "test_eval.es");
                res.ok().unwrap().get_string().clone()
            })
        });

        assert_eq!(res.as_str(), "i am eval");
    }

    #[test]
    fn test_report_exception() {
        use log::trace;
        //simple_logger::init().unwrap();

        let rt = crate::esruntime::tests::TEST_RT.clone();
        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    trace!("test_report_exception 2");

                    rooted!(in(cx) let mut rval = UndefinedValue());

                    trace!("test_report_exception 3");
                    let eval_res = jsapi_utils::eval(
                        rt,
                        global,
                        "let b = quibus * 12;",
                        "test_ex.es",
                        rval.handle_mut(),
                    );
                    trace!("test_report_exception 4");

                    if eval_res.is_err() {
                        let ex = eval_res.err().unwrap();

                        trace!("test_report_exception 6");
                        return ex.message;
                    }
                    trace!("test_report_exception 7");

                    "".to_string()
                })
            })
        });

        assert_eq!(res, "quibus is not defined");
    }
}
