use core::ptr;
use log::trace;
use mozjs::jsapi::CallArgs;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFreeOp;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSCLASS_FOREGROUND_FINALIZE;
use mozjs::jsval::ObjectValue;
use mozjs::rust::HandleObject;

// because we're using esvalue stuff here this should NOT be in es_utils

pub trait EsProxy<T> {
    //    fn construct(Vec<EsValueFacade>) -> T;
    //    fn call(obj: Option<T>, function_name: &str, args: Vec<EsValueFacade>) -> EsValueFacade;
    //    fn get(obj: Option<T>, prop_name: &str);
    //    fn set(obj: Option<T>, prop_name: &str, val: EsValueFacade);
    //    fn get_static_function_names() -> Vec<String>;
    //    fn get_static_getter_setter_names() -> Vec<String>;
    //    fn get_event_names() -> Vec<String>;
}

// todo impl a EsEvent trait?
//pub fn invoke_event(obj: T, event_name: &str, event_obj: String) -> Vec<EsValueFacade> {
//    panic!("NYI")
//}

/// create a class def in the runtime which constructs and calls methods in a rust proxy
///
pub fn reflect<T>(_scope: HandleObject, _name: &str, _proxy: Box<EsProxy<T>>) {}

#[cfg(test)]
mod tests {
    use crate::es_utils::reflection::*;
    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;

    #[test]
    fn test_reflection1() {
        log::info!("test_reflection1");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_res_esvf: EsValueFacade = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|_rt, cx, global| {
                    // create constructor here



                    crate::es_utils::functions::define_native_constructor(
                        cx,
                        global,
                        "MyTestClass",
                        Some(construct),
                    );

                    crate::es_utils::functions::define_native_constructor(
                        cx,
                        global,
                        "MyOtherClass",
                        Some(construct),
                    );

                });

                sm_rt
                    .eval(
                        "let obj = new MyTestClass(1, 'abc', true); let obj2 = new MyOtherClass(1, 'abc', true); 123;",
                        "test_reflection1.es",
                    )
                    .ok()
                    .unwrap()
            }))
        });

        assert_eq!(&123, test_res_esvf.get_i32())
    }
}

static ES_PROXY_CLASS_CLASS_OPS: JSClassOps = JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: None,
    mayResolve: None,
    finalize: Some(finalize),
    call: None,
    hasInstance: None,
    construct: None,
    trace: None,
};

static ES_PROXY_CLASS: JSClass = JSClass {
    name: b"EsProxy\0" as *const u8 as *const libc::c_char,
    flags: JSCLASS_FOREGROUND_FINALIZE,
    cOps: &ES_PROXY_CLASS_CLASS_OPS as *const JSClassOps,
    spec: ptr::null(),
    ext: ptr::null(),
    oOps: ptr::null(),
};

unsafe extern "C" fn finalize(_fop: *mut JSFreeOp, _object: *mut JSObject) {
    trace!("reflection::finalize");
}

unsafe extern "C" fn construct(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    trace!("reflection::construct");

    let args = CallArgs::from_vp(vp, argc);

    let thisv = args.calleev();

    rooted!(in (cx) let obj_root = thisv.to_object());

    let class_name =
        crate::es_utils::objects::get_es_obj_prop_val_as_string(cx, obj_root.handle(), "name");
    trace!("reflection::construct cn={}", class_name);

    let ret: *mut JSObject = mozjs::jsapi::JS_NewObject(cx, &ES_PROXY_CLASS);
    args.rval().set(ObjectValue(ret));

    true
}
