use crate::es_utils::{report_es_ex, EsErrorInfo};
use log::trace;
use mozjs::conversions::ConversionBehavior;
use mozjs::conversions::FromJSValConvertible;
use mozjs::conversions::ToJSValConvertible;
use mozjs::glue::int_to_jsid;
use mozjs::jsapi::IsArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_GetArrayLength;
use mozjs::jsapi::JS_GetPropertyById;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_SetElement;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::JSVal;
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

    let ok = unsafe { JS_GetArrayLength(context, arr_obj.into(), &mut l) };

    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }
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
    let idx = get_array_length(context, arr_obj).ok().unwrap();
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

    let id = unsafe { int_to_jsid(idx as i32) };
    rooted!(in (context) let id_root = id);

    let ok = unsafe {
        JS_GetPropertyById(
            context,
            arr_obj.into(),
            id_root.handle().into(),
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

pub fn new_array(context: *mut JSContext, items: Vec<JSVal>) -> *mut JSObject {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*items) };
    let res = unsafe { JS_NewArrayObject(context, &arguments_value_array) };

    res
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
    use crate::es_utils::functions::call_obj_method_name;
    use crate::es_utils::objects::get_es_obj_prop_val;
    use crate::es_utils::tests::test_with_sm_rt;
    use crate::es_utils::{es_value_to_str, report_es_ex};
    use mozjs::jsapi::JSObject;
    use mozjs::jsval::JSVal;
    use mozjs::jsval::UndefinedValue;
    use mozjs::jsval::{Int32Value, ObjectValue};

    #[test]
    fn test_is_array() {
        let res = test_with_sm_rt(|sm_rt| {
            println!("running arrays test");
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();

            rooted!(in (context) let global_root = global);
            println!("create array");
            let res = sm_rt.eval(
                "this.test_is_array = [4, 2, 3, 1]; this.test_is_array2 = {}; 123;",
                "test_is_array.es",
            );
            println!("created array");
            if res.is_err() {
                let err_res = report_es_ex(context);
                if let Some(err) = err_res {
                    println!("err {}", err.message);
                }
            }

            rooted!(in (context) let mut arr_val_root = UndefinedValue());
            rooted!(in (context) let mut arr_val2_root = UndefinedValue());

            let _res = get_es_obj_prop_val(
                context,
                global_root.handle(),
                "test_is_array",
                arr_val_root.handle_mut(),
            );
            let _res2 = get_es_obj_prop_val(
                context,
                global_root.handle(),
                "test_is_array2",
                arr_val2_root.handle_mut(),
            );

            rooted!(in (context) let arr_val_obj = arr_val_root.to_object());
            rooted!(in (context) let arr_val2_obj = arr_val2_root.to_object());

            assert_eq!(true, object_is_array(context, arr_val_obj.handle()));

            let length_res = get_array_length(context, arr_val_obj.handle());
            assert_eq!(4, length_res.ok().unwrap());

            assert_eq!(false, object_is_array(context, arr_val2_obj.handle()));

            rooted!(in (context) let mut rval = UndefinedValue());
            let res = get_array_element(context, arr_val_obj.handle(), 2, rval.handle_mut());
            if res.is_err() {
                panic!(res.err().unwrap().message);
            }
            let val_3: JSVal = *rval;
            assert_eq!(val_3.to_int32(), 3);

            true
        });

        assert_eq!(res, true);
    }

    #[test]
    fn test_create_array() {
        let res = test_with_sm_rt(|sm_rt| {
            println!("running arrays test");
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();

            rooted!(in (context) let global_root = global);

            rooted!(in (context) let v1_root = Int32Value(12));
            rooted!(in (context) let v2_root = Int32Value(15));

            let items: Vec<JSVal> = vec![*v1_root.handle(), *v2_root.handle()];

            let arr_obj: *mut JSObject = new_array(context, items);
            rooted!(in (context) let mut test_create_array_root = arr_obj);

            rooted!(in (context) let v3_root = Int32Value(7));
            let set_res = set_array_element(
                context,
                test_create_array_root.handle(),
                2,
                v3_root.handle(),
            );

            let length_res = get_array_length(context, test_create_array_root.handle());
            assert_eq!(3, length_res.ok().unwrap());

            rooted!(in (context) let v4_root = Int32Value(21));
            push_array_element(context, test_create_array_root.handle(), v4_root.handle());

            rooted!(in (context) let mut stringify_res_root = UndefinedValue());

            let test_create_array_val: JSVal = ObjectValue(arr_obj);

            call_obj_method_name(
                context,
                global_root.handle(),
                vec!["JSON"],
                "stringify",
                vec![test_create_array_val],
                &mut stringify_res_root.handle_mut(),
            )
            .ok()
            .unwrap();
            let stringify_res_str = es_value_to_str(context, &*stringify_res_root.handle());
            assert_eq!(stringify_res_str.as_str(), "[12,15,7,21]");

            true
        });

        assert_eq!(res, true);
    }
}
