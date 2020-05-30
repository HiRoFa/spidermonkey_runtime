use crate::es_utils::{report_es_ex, EsErrorInfo};
use log::trace;
use mozjs::conversions::ConversionBehavior;
use mozjs::conversions::FromJSValConvertible;
use mozjs::conversions::ToJSValConvertible;
use mozjs::glue::int_to_jsid;
use mozjs::jsapi::IsArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JS_GetArrayLength;
use mozjs::jsapi::JS_GetPropertyById;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_SetElement;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::{JSVal, ObjectValue};
use mozjs::rust::{HandleObject, HandleValue, MutableHandleValue};

/// check whether or not an Object is an Array
pub fn object_is_array(context: *mut JSContext, obj: HandleObject) -> bool {
    let mut is_array: bool = false;

    let ok = unsafe { IsArray(context, obj.into(), &mut is_array) };

    if !ok {
        if let Some(err) = report_es_ex(context) {
            trace!("error getting IsArray, ignoring: {}", err.message);
            return false;
        }
    }
    is_array
}

/// get the length of an Array
pub fn get_array_length(
    context: *mut JSContext,
    arr_obj: HandleObject,
) -> Result<u32, EsErrorInfo> {
    let mut l: u32 = 0;

    trace!("arrays::get_array_length");

    let ok = unsafe { JS_GetArrayLength(context, arr_obj.into(), &mut l) };

    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }
    trace!("arrays::get_array_length l={}", l);
    Ok(l)
}

pub fn set_array_element(
    context: *mut JSContext,
    arr_obj: HandleObject,
    idx: u32,
    val: HandleValue,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { JS_SetElement(context, arr_obj.into(), idx, val.into()) };
    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }

    Ok(())
}

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
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }

    Ok(())
}

/// create a new array obj
pub fn new_array(context: *mut JSContext, items: Vec<JSVal>, ret_val: &mut MutableHandleValue) {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*items) };
    let res = unsafe { JS_NewArrayObject(context, &arguments_value_array) };

    ret_val.set(ObjectValue(res));
}

/// convert an Array to a Vec<i32>
pub fn to_i32_vec(context: *mut JSContext, obj: HandleValue) -> Vec<i32> {
    let converted =
        unsafe { Vec::<i32>::from_jsval(context, obj, ConversionBehavior::Default) }.unwrap();
    // todo use mem_replace to return vec
    let vec_ref: &Vec<i32> = converted.get_success_value().unwrap();
    vec_ref.to_vec()
}

/// convert a Vec<i32> to an Array
pub fn to_i32_array<T>(context: *mut JSContext, obj: MutableHandleValue, vec: Vec<i32>) {
    unsafe { vec.to_jsval(context, obj) };
}

#[cfg(test)]
mod tests {
    use crate::es_utils::arrays::{
        get_array_element, get_array_length, new_array, object_is_array, push_array_element,
        set_array_element,
    };
    use crate::es_utils::functions::call_method_value;
    use crate::es_utils::objects::get_es_obj_prop_val;
    use crate::es_utils::tests::test_with_sm_rt;
    use crate::es_utils::{es_value_to_str, report_es_ex};
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
                    let err_res = report_es_ex(cx);
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
                    panic!(res.err().unwrap().message);
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

                rooted!(in (context) let mut array_rval = UndefinedValue());
                new_array(context, items, &mut array_rval.handle_mut());

                rooted!(in (context) let test_create_array_root = array_rval.get().to_object());

                rooted!(in (context) let v3_root = Int32Value(7));
                let _set_res = set_array_element(
                    context,
                    test_create_array_root.handle(),
                    2,
                    v3_root.handle(),
                );

                for _x in 0..100 {
                    let length_res = get_array_length(context, test_create_array_root.handle());
                    assert_eq!(3, length_res.ok().unwrap());
                }

                for _x in 0..11 {
                    rooted!(in (context) let v4_root = Int32Value(21));
                    push_array_element(context, test_create_array_root.handle(), v4_root.handle())
                        .ok()
                        .unwrap();
                }

                rooted!(in (context) let mut stringify_res_root = UndefinedValue());

                rooted!(in (context) let new_rooted_arr_val = ObjectValue(test_create_array_root.get()));

                rooted!(in (context) let mut stringify_func_root = UndefinedValue());

                rt.evaluate_script(global, "JSON.stringify.bind(JSON);", "get_stringify.es", 0, stringify_func_root.handle_mut()).ok().unwrap();

                call_method_value(context, global, stringify_func_root.handle(), vec![new_rooted_arr_val.get()], stringify_res_root.handle_mut()).ok().unwrap();
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
                assert_eq!(stringify_res_str.ok().unwrap().as_str(), "[12,15,7,21,21,21,21,21,21,21,21,21,21,21]");

                true
            })
        });

        assert_eq!(res, true);
    }
}
