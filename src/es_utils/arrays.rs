use mozjs::conversions::ConversionBehavior;
use mozjs::conversions::FromJSValConvertible;
use mozjs::conversions::ToJSValConvertible;
use mozjs::jsapi::IsArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JS_GetArrayLength;
use mozjs::jsapi::JS_GetPropertyById;
use log::trace;


use mozjs::rust::{HandleObject, HandleValue, MutableHandleValue};
use mozjs::glue::int_to_jsid;
use crate::es_utils::{EsErrorInfo, report_es_ex};

/// todo, these should return a Result<(something), EsErrorInfo>

/// check whether or not an Object is an Array
pub fn object_is_array(context: *mut JSContext, obj: HandleObject) -> bool {
    let mut is_array: bool = false;

    let ok = unsafe{IsArray(context, obj.into(), &mut is_array)};

    if !ok {
        if let Some(err) = report_es_ex(context) {
            trace!("error getting IsArray, ignoring: {}", err.message);
            return false;
        }
    }
    is_array
}

/// get the length of an Array
pub fn get_array_length(context: *mut JSContext, arr_obj: HandleObject) -> Result<u32, EsErrorInfo> {
   let mut l : u32 = 0;

    let ok = unsafe{JS_GetArrayLength(context, arr_obj.into(), &mut l)};

    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }
    Ok(l)
}

/// get the Value of an index of an Array
pub fn get_array_slot(context: *mut JSContext, arr_obj: HandleObject, idx: i32, ret_val: MutableHandleValue) -> Result<(), EsErrorInfo> {

    // could use https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_ValueToId
    // but the glue is here so thats easier

    let id = unsafe{int_to_jsid(idx)};
    rooted!(in (context) let id_root = id);

    let ok = unsafe{JS_GetPropertyById(context, arr_obj.into(), id_root.handle().into(), ret_val.into())};

    if !ok {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }

    Ok(())
}

/// convert an Array to a Vec<i32>
pub fn to_i32_vec(context: *mut JSContext, obj: HandleValue) -> Vec<i32> {
    let converted = unsafe {
        Vec::<i32>::from_jsval(context, obj,
                               ConversionBehavior::Default)
    }.unwrap();
    // todo use mem_replace to return vec
    let vec_ref: &Vec<i32> = converted.get_success_value().unwrap();
    vec_ref.to_vec()
}

/// convert a Vec<i32> to an Array
pub fn to_i32_array<T>(context: *mut JSContext, obj: MutableHandleValue, vec: Vec<i32>) {
    unsafe {vec.to_jsval( context, obj)};
}

#[cfg(test)]
mod tests {
    use crate::es_utils::tests::test_with_sm_rt;
    use crate::es_utils::report_es_ex;
    use crate::es_utils::arrays::{get_array_length, object_is_array, get_array_slot};
    use crate::es_utils::objects::get_es_obj_prop_val;
    use mozjs::jsval::JSVal;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_is_array(){

        let res = test_with_sm_rt(|sm_rt| {
            println!("running arrays test");
            let global = sm_rt.global_obj;
            let runtime = &sm_rt.runtime;
            let context = runtime.cx();

            rooted!(in (context) let global_root = global);
            println!("create array");
            let res = sm_rt.eval("this.test_is_array = [4, 2, 3, 1]; this.test_is_array2 = {}; 123;", "test_is_array.es");
            println!("created array");
            if res.is_err() {
                let err_res = report_es_ex(context);
                if let Some(err) = err_res {
                    println!("err {}", err.message);
                }
            }

            rooted!(in (context) let mut arr_val_root = UndefinedValue());
            rooted!(in (context) let mut arr_val2_root = UndefinedValue());

            let _res = get_es_obj_prop_val(context, global_root.handle(), "test_is_array", arr_val_root.handle_mut());
            let _res2 = get_es_obj_prop_val(context, global_root.handle(), "test_is_array2", arr_val2_root.handle_mut());

            rooted!(in (context) let arr_val_obj = arr_val_root.to_object());
            rooted!(in (context) let arr_val2_obj = arr_val2_root.to_object());

            assert_eq!(true, object_is_array(context,  arr_val_obj.handle()));

            let length_res = get_array_length(context, arr_val_obj.handle());
            assert_eq!(4, length_res.ok().unwrap());

            assert_eq!(false, object_is_array(context,  arr_val2_obj.handle()));

            rooted!(in (context) let mut rval = UndefinedValue());
            let res = get_array_slot(context, arr_val_obj.handle(), 2, rval.handle_mut());
            if res.is_err() {
                panic!(res.err().unwrap().message);
            }
            let val_3: JSVal = *rval;
            assert_eq!(val_3.to_int32(), 3);

            true
        });

        assert_eq!(res, true);

    }
}