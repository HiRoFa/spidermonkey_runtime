use crate::es_utils::{es_jsstring_to_string, es_value_to_str, report_es_ex, EsErrorInfo};
use mozjs::glue::RUST_JSID_IS_STRING;
use mozjs::glue::RUST_JSID_TO_STRING;
use mozjs::jsapi::HandleValueArray;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_GetConstructor;
use mozjs::jsapi::JS_GetProperty;
use mozjs::jsapi::JS_GetPrototype;
use mozjs::jsapi::JS_NewObjectWithGivenProto;
use mozjs::jsapi::JS_NewPlainObject;
use mozjs::jsapi::JSITER_OWNONLY;
use mozjs::jsval::{JSVal, UndefinedValue};
use mozjs::rust::jsapi_wrapped::GetPropertyKeys;
use mozjs::rust::wrappers::JS_DefineProperty;
use mozjs::rust::{
    HandleObject, HandleValue, IdVector, IntoHandle, MutableHandleObject, MutableHandleValue,
};
use std::ptr;

/// get a single member of a JSObject
#[allow(dead_code)]
pub fn get_es_obj_prop_val(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let n = format!("{}\0", prop_name);
    let ok = unsafe {
        JS_GetProperty(
            context,
            obj.into(),
            n.as_ptr() as *const libc::c_char,
            ret_val.into(),
        )
    };

    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }

    Ok(())
}

pub fn get_es_obj_prop_val_as_string(
    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,
) -> String {
    // todo in console we use something to convert any val to string, should we use that here or fail on non-strings?

    rooted!(in (context) let mut rval = UndefinedValue());
    let res = get_es_obj_prop_val(context, obj, prop_name, rval.handle_mut());
    if res.is_err() {
        panic!(res.err().unwrap().message);
    }

    es_value_to_str(context, &*rval)
}

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

/// constrcut a new object based on a constructor
pub fn new_from_constructor(
    context: *mut JSContext,
    constructor: HandleValue,
    args: HandleValueArray,
    ret_val: MutableHandleObject,
) -> Result<(), EsErrorInfo> {
    let ok =
        unsafe { mozjs::jsapi::Construct1(context, constructor.into(), &args, ret_val.into()) };
    if !ok {
        if let Some(err) = report_es_ex(context) {
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
        let err_opt = report_es_ex(context);
        if err_opt.is_some() {
            Err(err_opt.unwrap())
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

/// get all the propertynames of an object
#[allow(dead_code)]
pub fn get_js_obj_prop_names(context: *mut JSContext, obj: HandleObject) -> Vec<String> {
    let mut ids = unsafe { IdVector::new(context) };

    assert!(unsafe { GetPropertyKeys(context, obj.into(), JSITER_OWNONLY, ids.handle_mut()) });

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

#[cfg(test)]
mod tests {
    use crate::es_utils;
    use crate::es_utils::objects::{get_es_obj_prop_val, get_js_obj_prop_names};
    use crate::es_utils::{es_value_to_str, report_es_ex};
    use crate::esruntimewrapper::EsRuntimeWrapper;
    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::{JSVal, UndefinedValue};
    use std::borrow::Borrow;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_get_js_obj_prop_values() {
        use log::trace;
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    trace!("1");

                    rooted!(in(cx) let mut rval = UndefinedValue());
                    let _eval_res = es_utils::eval(
                        rt,
                        global,
                        "({a: '1', b: '2', c: '3'})",
                        "test_get_js_obj_prop_values.es",
                        rval.handle_mut(),
                    );

                    trace!("4");

                    let e_opt = report_es_ex(cx);
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

                        test_vec.push(es_value_to_str(cx, &*rval));

                        trace!("9 {}", prop_name);
                    }

                    trace!("10");

                    test_vec
                })
            }))
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
        use mozjs::jsapi::JSObject;

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let test_vec = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|rt, cx, global| {
                    rooted!(in(cx) let mut rval = UndefinedValue());
                    let eval_res = es_utils::eval(
                        rt,
                        global,
                        "({a: 1, b: 2, c: 3});",
                        "test_get_js_obj_prop_names.es",
                        rval.handle_mut(),
                    );

                    if eval_res.is_err() {
                        let e_opt = report_es_ex(cx);
                        assert!(e_opt.is_none());
                    }

                    let val: JSVal = *rval;
                    let jso: *mut JSObject = val.to_object();
                    rooted!(in (cx) let jso_root = jso);

                    get_js_obj_prop_names(cx, jso_root.handle())
                })
            }))
        });

        assert_eq!(test_vec.len(), 3);
        assert_eq!(test_vec.get(0).unwrap(), &"a".to_string());
        assert_eq!(test_vec.get(1).unwrap(), &"b".to_string());
        assert_eq!(test_vec.get(2).unwrap(), &"c".to_string());
    }

    #[test]
    fn test_get_obj_props() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.eval_sync("({a: 1, b: 'abc', c: true, d: 'much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string'});", "test_get_obj_props");
        assert!(res.is_ok());
        let map = res.ok().unwrap();
        let map: &HashMap<String, EsValueFacade> = map.get_object();
        assert_eq!(map.get(&"b".to_string()).unwrap().get_string(), "abc");
    }

    #[test]
    fn test_constructor() {
        use mozjs::jsapi::HandleValueArray;
        use mozjs::jsval::NullValue;

        let rt_arc: Arc<EsRuntimeWrapper> = crate::esruntimewrapper::tests::TEST_RT.clone();
        let rt: &EsRuntimeWrapper = rt_arc.borrow();
        let ok = rt.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in (cx) let mut constructor_root = UndefinedValue());
                let eval_res = es_utils::eval(
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
                es_utils::objects::new_from_constructor(
                    cx,
                    constructor_root.handle(),
                    args,
                    b_instance_root.handle_mut(),
                )
                .ok()
                .unwrap();

                rooted!(in (cx) let mut ret_val = UndefinedValue());
                es_utils::functions::call_method_name(
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
