use crate::jsapi_utils::{
    es_jsstring_to_string, es_value_to_str, get_pending_exception, EsErrorInfo,
};
use log::trace;
use mozjs::glue::RUST_JSID_IS_STRING;
use mozjs::glue::RUST_JSID_TO_STRING;
use mozjs::jsapi::HandleObject as RawHandleObject;
use mozjs::jsapi::HandleValue as RawHandleValue;
use mozjs::jsapi::HandleValueArray;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_DefineProperty;
use mozjs::jsapi::JS_GetConstructor;
use mozjs::jsapi::JS_GetProperty;
use mozjs::jsapi::JS_GetPrototype;
use mozjs::jsapi::JS_NewObjectWithGivenProto;
use mozjs::jsapi::JS_NewPlainObject;
use mozjs::jsapi::JSITER_OWNONLY;
use mozjs::jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::jsapi_wrapped::GetPropertyKeys;
use mozjs::rust::{
    HandleObject, HandleValue, IdVector, IntoHandle, MutableHandleObject, MutableHandleValue,
};
use std::ptr;

/// get a namespace object and create any part that is not yet defined
pub fn get_or_define_namespace(
    context: *mut JSContext,
    obj: HandleObject,
    namespace: Vec<&str>,
) -> *mut JSObject {
    trace!("get_or_define_package");

    let mut cur_obj = *obj;
    for name in namespace {
        trace!("get_or_define_package, loop step: {}", name);

        rooted!(in(context) let mut cur_root = cur_obj);

        rooted!(in(context) let mut sub_root = UndefinedValue());
        get_es_obj_prop_val(context, cur_root.handle(), name, sub_root.handle_mut())
            .ok()
            .unwrap();

        if sub_root.is_null_or_undefined() {
            trace!("get_or_define_package, loop step: {} is null, create", name);
            // create
            let new_obj = new_object(context);
            trace!(
                "get_or_define_package, loop step: {} is null, created",
                name
            );
            rooted!(in(context) let mut new_obj_val_root = ObjectValue(new_obj));
            set_es_obj_prop_value(context, cur_root.handle(), name, new_obj_val_root.handle());
            trace!(
                "get_or_define_package, loop step: {} is null, prop_set",
                name
            );
            cur_obj = new_obj;
        } else {
            trace!("get_or_define_package, loop step: {} exists", name);
            cur_obj = sub_root.to_object();
        }
    }

    cur_obj
}

/// get a single member of a JSObject
#[allow(dead_code)]
pub fn get_es_obj_prop_val(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    get_es_obj_prop_val_raw(context, obj.into(), prop_name, ret_val)
}

/// get a single member of a JSObject
#[allow(dead_code)]
pub fn get_es_obj_prop_val_raw(
    context: *mut JSContext,
    obj: RawHandleObject,
    prop_name: &str,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let n = format!("{}\0", prop_name);
    let ok = unsafe {
        JS_GetProperty(
            context,
            obj,
            n.as_ptr() as *const libc::c_char,
            ret_val.into(),
        )
    };

    if !ok {
        if let Some(err) = get_pending_exception(context) {
            return Err(err);
        }
    }

    Ok(())
}

/// util method to quickly get a property of a JSObject as String
pub fn get_es_obj_prop_val_as_string(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
) -> Result<String, &'static str> {
    rooted!(in (context) let mut rval = UndefinedValue());
    let res = get_es_obj_prop_val(context, obj, prop_name, rval.handle_mut());
    if res.is_err() {
        panic!(res.err().unwrap().message);
    }

    es_value_to_str(context, *rval)
}

/// util method to quickly get a property of a JSObject as String
pub fn get_es_obj_prop_val_as_string_raw(
    context: *mut JSContext,
    obj: RawHandleObject,
    prop_name: &str,
) -> Result<String, &'static str> {
    rooted!(in (context) let mut rval = UndefinedValue());
    let res = get_es_obj_prop_val_raw(context, obj, prop_name, rval.handle_mut());
    if res.is_err() {
        panic!(res.err().unwrap().message);
    }

    es_value_to_str(context, *rval)
}

/// util method to quickly get a property of a JSObject as i32
pub fn get_es_obj_prop_val_as_i32(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
) -> i32 {
    rooted!(in (context) let mut rval = UndefinedValue());
    let res = get_es_obj_prop_val(context, obj, prop_name, rval.handle_mut());
    if res.is_err() {
        panic!(res.err().unwrap().message);
    }

    let val: JSVal = *rval;
    val.to_int32()
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
        let err_opt = get_pending_exception(context);
        if let Some(err) = err_opt {
            Err(err)
        } else {
            Ok(ret)
        }
    } else {
        Ok(ret)
    }
}

/// construct a new object based on a constructor
pub fn new_from_constructor(
    context: *mut JSContext,
    constructor: HandleValue,
    args: HandleValueArray,
    ret_val: MutableHandleObject,
) -> Result<(), EsErrorInfo> {
    let ok =
        unsafe { mozjs::jsapi::Construct1(context, constructor.into(), &args, ret_val.into()) };
    if !ok {
        if let Some(err) = get_pending_exception(context) {
            return Err(err);
        }
    }
    Ok(())
}

static CLASS: JSClass = JSClass {
    name: b"EventTargetPrototype\0" as *const u8 as *const libc::c_char,
    flags: 0,
    cOps: 0 as *const _,
    spec: ptr::null(),
    ext: ptr::null(),
    oOps: ptr::null(),
};

/// get the prototype of an object
#[allow(dead_code)]
pub fn get_prototype(
    context: *mut JSContext,
    obj: HandleObject,
    ret_val: MutableHandleObject,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { JS_GetPrototype(context, obj.into_handle(), ret_val.into()) };

    if !ok {
        let err_opt = get_pending_exception(context);
        if let Some(err) = err_opt {
            Err(err)
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

/// get the constructor of an object
#[allow(dead_code)]
pub fn get_constructor(
    context: *mut JSContext,
    obj: HandleObject,
) -> Result<*mut JSObject, EsErrorInfo> {
    let ret: *mut JSObject = unsafe { JS_GetConstructor(context, obj.into_handle()) };

    // todo rebuild with ret_val: MutableHandle instead of returning Value

    if ret.is_null() {
        let err_opt = get_pending_exception(context);
        if let Some(err) = err_opt {
            Err(err)
        } else {
            Ok(ret)
        }
    } else {
        Ok(ret)
    }
}

/// get all the propertynames of an object
#[allow(dead_code)]
pub fn get_js_obj_prop_names(context: *mut JSContext, obj: HandleObject) -> Vec<String> {
    let mut ids = unsafe { IdVector::new(context) };

    assert!(unsafe { GetPropertyKeys(context, obj, JSITER_OWNONLY, ids.handle_mut()) });

    let mut ret: Vec<String> = vec![];

    for x in 0..ids.len() {
        rooted!(in(context) let id = ids[x]);
        assert!(unsafe { RUST_JSID_IS_STRING(id.handle().into()) });
        rooted!(in(context) let id_str = unsafe{RUST_JSID_TO_STRING(id.handle().into())});
        let prop_name = es_jsstring_to_string(context, *id_str);
        ret.push(prop_name);
    }
    ret
}

/// set a property of an object
#[allow(dead_code)]
pub fn set_es_obj_prop_value_raw(
    context: *mut JSContext,
    obj: RawHandleObject,
    prop_name: &str,
    prop_val: RawHandleValue,
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

/// set a property of an object
#[allow(dead_code)]
pub fn set_es_obj_prop_value(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
    prop_val: HandleValue,
) {
    let prop_name_str = format!("{}\0", prop_name);
    unsafe {
        JS_DefineProperty(
            context,
            obj.into(),
            prop_name_str.as_ptr() as *const libc::c_char,
            prop_val.into(),
            mozjs::jsapi::JSPROP_ENUMERATE as u32,
        );
    }
}

/// set a property of an object
#[allow(dead_code)]
pub fn set_es_obj_prop_val_permanent(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
    prop_val: HandleValue,
) {
    let prop_name_str = format!("{}\0", prop_name);
    unsafe {
        JS_DefineProperty(
            context,
            obj.into(),
            prop_name_str.as_ptr() as *const libc::c_char,
            prop_val.into(),
            (mozjs::jsapi::JSPROP_PERMANENT & mozjs::jsapi::JSPROP_READONLY) as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::esruntime::EsRuntime;
    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils;
    use crate::jsapi_utils::objects::{
        get_es_obj_prop_val, get_js_obj_prop_names, get_or_define_namespace,
    };
    use crate::jsapi_utils::{es_value_to_str, get_pending_exception};
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::{JSVal, UndefinedValue};
    use std::borrow::Borrow;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_get_js_obj_prop_values() {
        log::info!("test: test_get_js_obj_prop_values");
        use log::trace;
        let rt = crate::esruntime::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    trace!("1");

                    rooted!(in(cx) let mut rval = UndefinedValue());
                    let _eval_res = jsapi_utils::eval(
                        rt,
                        global,
                        "({a: '1', b: '2', c: '3'})",
                        "test_get_js_obj_prop_values.es",
                        rval.handle_mut(),
                    );

                    trace!("4");

                    let e_opt = get_pending_exception(cx);
                    assert!(e_opt.is_none());

                    trace!("5");

                    let jso = rval.to_object();
                    rooted!(in(cx) let jso_root = jso);

                    trace!("6");

                    let prop_vec: Vec<String> = get_js_obj_prop_names(cx, jso_root.handle());

                    trace!("7");

                    let mut test_vec = vec![];

                    trace!("8");

                    for prop_name in prop_vec {
                        rooted!(in (cx) let mut rval = UndefinedValue());

                        let _prop_val_res = get_es_obj_prop_val(
                            cx,
                            jso_root.handle(),
                            prop_name.as_str(),
                            rval.handle_mut(),
                        );

                        test_vec.push(es_value_to_str(cx, *rval).ok().unwrap());

                        trace!("9 {}", prop_name);
                    }

                    trace!("10");

                    test_vec
                })
            })
        });

        assert_eq!(test_vec.len(), 3);
        assert_eq!(test_vec.get(0).unwrap(), &"1".to_string());
        assert_eq!(test_vec.get(1).unwrap(), &"2".to_string());
        assert_eq!(test_vec.get(2).unwrap(), &"3".to_string());
    }

    #[test]
    fn test_get_js_obj_prop_names_x() {
        for _x in 0..10 {
            test_get_js_obj_prop_names();
        }
    }

    #[test]
    fn test_get_js_obj_prop_names() {
        log::info!("test: test_get_js_obj_prop_names");
        use mozjs::jsapi::JSObject;

        let rt = crate::esruntime::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    rooted!(in(cx) let mut rval = UndefinedValue());
                    let eval_res = jsapi_utils::eval(
                        rt,
                        global,
                        "({a: 1, b: 2, c: 3});",
                        "test_get_js_obj_prop_names.es",
                        rval.handle_mut(),
                    );

                    if eval_res.is_err() {
                        let e_opt = get_pending_exception(cx);
                        assert!(e_opt.is_none());
                    }

                    let val: JSVal = *rval;
                    let jso: *mut JSObject = val.to_object();
                    rooted!(in (cx) let jso_root = jso);

                    get_js_obj_prop_names(cx, jso_root.handle())
                })
            })
        });

        assert_eq!(test_vec.len(), 3);
        assert_eq!(test_vec.get(0).unwrap(), &"a".to_string());
        assert_eq!(test_vec.get(1).unwrap(), &"b".to_string());
        assert_eq!(test_vec.get(2).unwrap(), &"c".to_string());
    }

    #[test]
    fn test_get_or_define_package() {
        log::info!("test: test_get_or_define_package");
        let rt_arc: Arc<EsRuntime> = crate::esruntime::tests::TEST_RT.clone();
        let rt: &EsRuntime = rt_arc.borrow();
        let res = rt.do_in_es_event_queue_sync(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, cx, global| {
                get_or_define_namespace(cx, global, vec!["test_get_or_define_package", "a", "b"]);
                get_or_define_namespace(cx, global, vec!["test_get_or_define_package", "a", "c"]);
            });

            true
        });
        assert_eq!(res, true);

        let res = rt
            .eval_sync(
                "JSON.stringify(test_get_or_define_package);",
                "test_get_or_define_package.es",
            )
            .ok()
            .unwrap();

        let json = res.get_string();
        let expect = "{\"a\":{\"b\":{},\"c\":{}}}";
        assert_eq!(json, expect);
    }

    #[test]
    fn test_get_obj_props() {
        log::info!("test: test_get_obj_props");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let res = rt.eval_sync("({a: 1, b: 'abc', c: true, d: 'much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string'});", "test_get_obj_props");
        assert!(res.is_ok());
        let map = res.ok().unwrap();
        let map: &HashMap<String, EsValueFacade> = map.get_object();
        assert_eq!(map.get(&"b".to_string()).unwrap().get_string(), "abc");
    }

    #[test]
    fn test_constructor() {
        log::info!("test: test_constructor");
        use mozjs::jsapi::HandleValueArray;
        use mozjs::jsval::NullValue;

        let rt_arc: Arc<EsRuntime> = crate::esruntime::tests::TEST_RT.clone();
        let rt: &EsRuntime = rt_arc.borrow();
        let ok = rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in (cx) let mut constructor_root = UndefinedValue());
                let eval_res = jsapi_utils::eval(
                    rt,
                    global,
                    "(class MyClass {constructor(){this.b = 5;} a(){return this.b;}});",
                    "test_constructor.es",
                    constructor_root.handle_mut(),
                );
                if eval_res.is_err() {
                    panic!("eval failed: {}", eval_res.err().unwrap().err_msg());
                }

                rooted!(in (cx) let mut b_instance_root = NullValue().to_object_or_null());
                let args = HandleValueArray::new();
                jsapi_utils::objects::new_from_constructor(
                    cx,
                    constructor_root.handle(),
                    args,
                    b_instance_root.handle_mut(),
                )
                .ok()
                .unwrap();

                rooted!(in (cx) let mut ret_val = UndefinedValue());
                jsapi_utils::functions::call_method_name(
                    cx,
                    b_instance_root.handle(),
                    "a",
                    vec![],
                    ret_val.handle_mut(),
                )
                .ok()
                .unwrap();

                let val: JSVal = *ret_val;

                let i: i32 = val.to_int32();

                assert_eq!(i, 5);
                true
            })
        });
        assert_eq!(ok, true);
    }
}
