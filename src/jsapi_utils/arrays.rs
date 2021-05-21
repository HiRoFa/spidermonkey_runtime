use crate::jsapi_utils::{
    get_pending_exception, get_pending_exception_or_generic_err, EsErrorInfo,
};
use log::trace;
use mozjs::conversions::{
    ConversionBehavior, ConversionResult, FromJSValConvertible, ToJSValConvertible,
};
use mozjs::glue::int_to_jsid;
use mozjs::jsapi::GetArrayLength;
use mozjs::jsapi::IsArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_GetPropertyById;
use mozjs::jsapi::JS_SetElement;
use mozjs::jsapi::NewArrayObject;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::JSVal;
use mozjs::rust::{HandleObject, HandleValue, MutableHandleObject, MutableHandleValue};

/// convert an Array to a Vec<T>, should work for all which impl the FromJSValConvertible trait like:
/// bool
/// u8
/// i8
/// u16
/// i16
/// u32
/// i32
/// u64
/// i64
/// f32
/// f64
/// String
/// # Example
/// ```no_run
/// use spidermonkey_runtime::esruntimebuilder::EsRuntimeBuilder;
/// use spidermonkey_runtime::jsapi_utils;
/// use mozjs::jsval::UndefinedValue;
/// use mozjs::rooted;
/// use spidermonkey_runtime::jsapi_utils::arrays::array_to_vec;
/// let rt = EsRuntimeBuilder::new().build();
/// rt.do_in_es_event_queue_sync(|sm_rt| {
/// sm_rt.do_with_jsapi(|rt, cx, global| {
///  rooted!(in (cx) let mut rval = UndefinedValue());
///  jsapi_utils::eval(rt, global, "([4, 7, 9]);", "test_array_to_vec.es", rval.handle_mut());
///  let conv_res = array_to_vec(cx, rval.handle());
///  let conv_res_sucv = conv_res.get_success_value();
///  let vec: &Vec<u8> = conv_res_sucv.expect("array_to_vec failed");
///  assert_eq!(vec.len(), 3);
/// })
/// });
/// ```
pub fn array_to_vec<T: FromJSValConvertible<Config = ConversionBehavior>>(
    context: *mut JSContext,
    arr_val_handle: HandleValue,
) -> ConversionResult<Vec<T>> {
    unsafe { Vec::<T>::from_jsval(context, arr_val_handle, ConversionBehavior::Default) }.unwrap()
}

/// convert a Vec<T> to an Array, please note that this does not create typed arrays, use the typed_arrays mod for that
///
/// should work for all which impl the FromJSValConvertible trait like:
/// bool
/// u8
/// i8
/// u16
/// i16
/// u32
/// i32
/// u64
/// i64
/// f32
/// f64
/// String
/// # Example
/// ```no_run
/// use spidermonkey_runtime::esruntimebuilder::EsRuntimeBuilder;
/// use spidermonkey_runtime::jsapi_utils;
/// use mozjs::rooted;
/// use mozjs::jsval::UndefinedValue;
/// use spidermonkey_runtime::jsapi_utils::arrays::{array_to_vec, vec_to_array, get_array_length};
/// use mozjs::rust::HandleObject;
///   
/// let vec = vec![2, 6, 0, 12];
///
/// let rt = EsRuntimeBuilder::new().build();
/// rt.do_in_es_event_queue_sync(|sm_rt| {
/// sm_rt.do_with_jsapi(|rt, cx, global| {
///  rooted!(in (cx) let mut rval = UndefinedValue());
///  vec_to_array(cx, rval.handle_mut(), vec);
///  let arr_obj_handle = unsafe{HandleObject::from_marked_location(&rval.handle().get().to_object())};
///  let arr_len = get_array_length(cx, arr_obj_handle).ok().expect("could not get array length");
///  assert_eq!(arr_len, 4);
/// })
/// });
/// ```
pub fn vec_to_array<T: ToJSValConvertible>(
    context: *mut JSContext,
    obj: MutableHandleValue,
    vec: Vec<T>,
) {
    unsafe { vec.to_jsval(context, obj) };
}

/// check whether or not an Object is an Array
pub fn object_is_array(context: *mut JSContext, obj: HandleObject) -> bool {
    let mut is_array: bool = false;

    let ok = unsafe { IsArray(context, obj.into(), &mut is_array) };

    if !ok {
        if let Some(err) = get_pending_exception(context) {
            trace!("error getting IsArray, ignoring: {}", err.message);
            return false;
        }
    }
    is_array
}

/// check whether or not an Object is an Array
pub fn object_is_array2(context: *mut JSContext, obj: *mut JSObject) -> bool {
    rooted!(in (context) let obj_root = obj);

    object_is_array(context, obj_root.handle())
}

/// get the length of an Array
pub fn get_array_length(
    context: *mut JSContext,
    arr_obj: HandleObject,
) -> Result<u32, EsErrorInfo> {
    let mut l: u32 = 0;

    trace!("arrays::get_array_length");

    let ok = unsafe { GetArrayLength(context, arr_obj.into(), &mut l) };

    if !ok {
        if let Some(err) = get_pending_exception(context) {
            return Err(err);
        }
    }
    trace!("arrays::get_array_length l={}", l);
    Ok(l)
}

/// set an element of an Array
pub fn set_array_element(
    cx: *mut JSContext,
    arr_obj: HandleObject,
    idx: u32,
    val: HandleValue,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { JS_SetElement(cx, arr_obj.into(), idx, val.into()) };
    if !ok {
        return Err(get_pending_exception_or_generic_err(
            cx,
            "failed to set_array_element",
        ));
    }

    Ok(())
}

/// set an element of an Array
pub fn set_array_element_object(
    cx: *mut JSContext,
    arr_obj: HandleObject,
    idx: u32,
    val: HandleObject,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { mozjs::jsapi::JS_SetElement1(cx, arr_obj.into(), idx, val.into()) };
    if !ok {
        return Err(get_pending_exception_or_generic_err(
            cx,
            "failed to set_array_element_object",
        ));
    }

    Ok(())
}

pub fn set_array_element_i32(
    cx: *mut JSContext,
    arr_obj: HandleObject,
    index: u32,
    val: i32,
) -> Result<(), EsErrorInfo> {
    let ret = unsafe { mozjs::jsapi::JS_SetElement3(cx, arr_obj.into(), index, val) };
    if !ret {
        return Err(get_pending_exception_or_generic_err(
            cx,
            "failed to set_array_element_i32",
        ));
    }
    Ok(())
}

pub fn set_array_element_u32(
    cx: *mut JSContext,
    arr_obj: HandleObject,
    index: u32,
    val: u32,
) -> Result<(), EsErrorInfo> {
    let ret = unsafe { mozjs::jsapi::JS_SetElement4(cx, arr_obj.into(), index, val) };
    if !ret {
        return Err(get_pending_exception_or_generic_err(
            cx,
            "failed to set_array_element_u32",
        ));
    }
    Ok(())
}

pub fn set_array_element_f64(
    cx: *mut JSContext,
    arr_obj: HandleObject,
    index: u32,
    val: f64,
) -> Result<(), EsErrorInfo> {
    let ret = unsafe { mozjs::jsapi::JS_SetElement5(cx, arr_obj.into(), index, val) };
    if !ret {
        return Err(get_pending_exception_or_generic_err(
            cx,
            "failed to set_array_element_f64",
        ));
    }
    Ok(())
}

/// add an element to an Array
pub fn push_array_element(
    context: *mut JSContext,
    arr_obj: HandleObject,
    val: HandleValue,
) -> Result<(), EsErrorInfo> {
    trace!("arrays::push_array_element");
    let idx = get_array_length(context, arr_obj).ok().unwrap();
    trace!("arrays::push_array_element, idx={}", idx);
    set_array_element(context, arr_obj, idx, val)
}

/// get the Value of an index of an Array
pub fn get_array_element(
    context: *mut JSContext,
    arr_obj: HandleObject,
    idx: u32,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    // could use https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_ValueToId
    // but the glue is here so thats easier

    rooted!(in (context) let mut mutable_handle_id = mozjs::jsapi::PropertyKey::default());
    unsafe { int_to_jsid(idx as i32, mutable_handle_id.handle_mut().into()) };

    let ok = unsafe {
        JS_GetPropertyById(
            context,
            arr_obj.into(),
            mutable_handle_id.handle().into(),
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

/// create a new array obj
pub fn new_array(context: *mut JSContext, ret_val: MutableHandleObject) {
    let arguments_value_array = HandleValueArray::new();
    let res = unsafe { NewArrayObject(context, &arguments_value_array) };
    let mut ret_val = ret_val;
    ret_val.set(res);
}

/// create a new array obj
pub fn new_array2(context: *mut JSContext, items: Vec<JSVal>, ret_val: MutableHandleObject) {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*items) };
    let res = unsafe { NewArrayObject(context, &arguments_value_array) };
    let mut ret_val = ret_val;
    ret_val.set(res);
}

#[cfg(test)]
mod tests {
    use crate::jsapi_utils::arrays::{
        get_array_element, get_array_length, new_array2, object_is_array, push_array_element,
        set_array_element,
    };
    use crate::jsapi_utils::functions::call_function_value;
    use crate::jsapi_utils::objects::get_es_obj_prop_val;
    use crate::jsapi_utils::objects::NULL_JSOBJECT;
    use crate::jsapi_utils::tests::test_with_sm_rt;
    use crate::jsapi_utils::{es_value_to_str, get_pending_exception};
    use mozjs::jsval::JSVal;
    use mozjs::jsval::UndefinedValue;
    use mozjs::jsval::{Int32Value, ObjectValue};

    #[test]
    fn test_is_array() {
        log::info!("test: test_is_array");
        let res = test_with_sm_rt(|sm_rt| {
            println!("running arrays test");
            sm_rt.do_with_jsapi(|_rt, cx, global| {
                println!("create array");
                let res = sm_rt.eval(
                    "this.test_is_array = [4, 2, 3, 1]; this.test_is_array2 = {}; 123;",
                    "test_is_array.es",
                );
                println!("created array");
                if res.is_err() {
                    let err_res = get_pending_exception(cx);
                    if let Some(err) = err_res {
                        println!("err {}", err.message);
                    }
                }

                rooted!(in (cx) let mut arr_val_root = UndefinedValue());
                rooted!(in (cx) let mut arr_val2_root = UndefinedValue());

                let _res =
                    get_es_obj_prop_val(cx, global, "test_is_array", arr_val_root.handle_mut());
                let _res2 =
                    get_es_obj_prop_val(cx, global, "test_is_array2", arr_val2_root.handle_mut());

                rooted!(in (cx) let arr_val_obj = arr_val_root.to_object());
                rooted!(in (cx) let arr_val2_obj = arr_val2_root.to_object());

                assert_eq!(true, object_is_array(cx, arr_val_obj.handle()));

                let length_res = get_array_length(cx, arr_val_obj.handle());
                assert_eq!(4, length_res.ok().unwrap());

                assert_eq!(false, object_is_array(cx, arr_val2_obj.handle()));

                rooted!(in (cx) let mut rval = UndefinedValue());
                let res = get_array_element(cx, arr_val_obj.handle(), 2, rval.handle_mut());
                if res.is_err() {
                    panic!("{}", res.err().unwrap().message);
                }
                let val_3: JSVal = *rval;
                assert_eq!(val_3.to_int32(), 3);

                true
            })
        });

        assert_eq!(res, true);
    }

    #[test]
    fn test_create_array() {
        log::info!("test: test_create_array");
        let res = test_with_sm_rt(|sm_rt| {
            println!("running arrays test");

            sm_rt.do_with_jsapi(|rt, context, global| {
                rooted!(in (context) let v1_root = Int32Value(12));
                rooted!(in (context) let v2_root = Int32Value(15));

                let items: Vec<JSVal> = vec![*v1_root.handle(), *v2_root.handle()];

                rooted!(in (context) let mut array_rval = NULL_JSOBJECT);
                new_array2(context, items, array_rval.handle_mut());

                rooted!(in (context) let v3_root = Int32Value(7));
                let _set_res = set_array_element(context, array_rval.handle(), 2, v3_root.handle());

                for _x in 0..100 {
                    let length_res = get_array_length(context, array_rval.handle());
                    assert_eq!(3, length_res.ok().unwrap());
                }

                for _x in 0..11 {
                    rooted!(in (context) let v4_root = Int32Value(21));
                    push_array_element(context, array_rval.handle(), v4_root.handle())
                        .ok()
                        .unwrap();
                }

                rooted!(in (context) let mut stringify_res_root = UndefinedValue());

                rooted!(in (context) let mut stringify_func_root = UndefinedValue());

                rt.evaluate_script(
                    global,
                    "JSON.stringify.bind(JSON);",
                    "get_stringify.es",
                    0,
                    stringify_func_root.handle_mut(),
                )
                .ok()
                .unwrap();

                call_function_value(
                    context,
                    global,
                    stringify_func_root.handle(),
                    vec![ObjectValue(array_rval.get())],
                    stringify_res_root.handle_mut(),
                )
                .ok()
                .unwrap();
                /*
                // tddo, why does this cause a invalid mem ref when testing with gc_ZEAL
                call_obj_method_name(
                    context,
                    global,
                    vec!["JSON"],
                    "stringify",
                    vec![new_rooted_arr_val.get()],
                    stringify_res_root.handle_mut(),
                )
                .ok()
                .unwrap();
                */

                let stringify_res_str = es_value_to_str(context, *stringify_res_root.handle());
                assert_eq!(
                    stringify_res_str.ok().unwrap().as_str(),
                    "[12,15,7,21,21,21,21,21,21,21,21,21,21,21]"
                );

                true
            })
        });

        assert_eq!(res, true);
    }
}
