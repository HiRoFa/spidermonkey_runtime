use mozjs::rust::{HandleObject, HandleValue, IntoHandle, IdVector, MutableHandleValue};
use mozjs::jsapi::JSContext;
use crate::es_utils::{EsErrorInfo, report_es_ex, es_jsstring_to_string, es_value_to_str};
use mozjs::jsval::{UndefinedValue, JSVal};
use mozjs::jsapi::JS_GetProperty;
use mozjs::jsapi::JSObject;
use mozjs::rust::jsapi_wrapped::GetPropertyKeys;
use mozjs::rust::wrappers::JS_DefineProperty;
use mozjs::jsapi::JS_NewObjectWithGivenProto;
use mozjs::jsapi::JS_NewPlainObject;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSCLASS_FOREGROUND_FINALIZE;
use mozjs::jsapi::JSITER_OWNONLY;
use mozjs::jsapi::JS_GetConstructor;
use mozjs::jsapi::JS_GetObjectPrototype;
use mozjs::glue::RUST_JSID_IS_STRING;
use mozjs::glue::RUST_JSID_TO_STRING;

/// get a single member of a JSObject
#[allow(dead_code)]
pub fn get_es_obj_prop_val(

    context: *mut JSContext,
    obj: HandleObject,
    prop_name: &str,ret_val: MutableHandleValue) -> Result<(), EsErrorInfo> {

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
    prop_name: &str) -> String {

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
    prop_name: &str) -> i32 {

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

        let prop_name = es_jsstring_to_string(context, *id);

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
    use crate::es_utils::{report_es_ex, es_value_to_str};
    use crate::es_utils::objects::{get_es_obj_prop_val, get_js_obj_prop_names};
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::UndefinedValue;
    use crate::esvaluefacade::EsValueFacade;
    use std::collections::HashMap;


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
                rooted!(in(context) let jso_root = jso);

                for prop_name in prop_vec {

                    rooted!(in (context) let mut rval = UndefinedValue());

                    let _prop_val_res = get_es_obj_prop_val(context, jso_root.handle(), prop_name.as_str(), rval.handle_mut());

                    test_vec.push(es_value_to_str(context, &*rval).to_string());
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
    fn test_get_obj_props() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let res = rt.eval_sync("({a: 1, b: 'abc', c: true, d: 'much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string much larger string'});", "test_get_obj_props");
        assert!(res.is_ok());
        let map = res.ok().unwrap();
        let map: &HashMap<String, EsValueFacade> = map.get_object();
        assert_eq!(map.get(&"b".to_string()).unwrap().get_string(), "abc");
    }

}