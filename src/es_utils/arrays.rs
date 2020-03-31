use mozjs::conversions::ConversionBehavior;
use mozjs::conversions::FromJSValConvertible;
use mozjs::conversions::ToJSValConvertible;
use mozjs::jsapi::IsArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JS_GetArrayLength;


use mozjs::rust::{HandleObject, HandleValue, MutableHandleValue};


pub fn object_is_array(context: *mut JSContext, obj: HandleObject) -> bool {
    let mut is_array: bool = false;
    unsafe {
        IsArray(context, obj.into(), &mut is_array);
    };
    is_array
}

pub fn get_array_length(context: *mut JSContext, arr_obj: HandleObject) -> u32 {
   let mut l : u32 = 0;
    unsafe{
        JS_GetArrayLength(context, arr_obj.into(), &mut l);
    };
    l
}

pub fn get_array_slot(_context: *mut JSContext, _arr_obj: HandleObject, _idx: u32, _ret_val: MutableHandleValue) {
    panic!("NYI");
}

pub fn to_i32_vec(context: *mut JSContext, obj: HandleValue) -> Vec<i32> {
    let converted = unsafe {
        Vec::<i32>::from_jsval(context, obj,
                               ConversionBehavior::Default)
    }.unwrap();
    // todo use mem_replace to return vec
    let vec_ref: &Vec<i32> = converted.get_success_value().unwrap();
    vec_ref.to_vec()
}

pub fn to_i32_array<T>(context: *mut JSContext, obj: MutableHandleValue, vec: Vec<i32>) {
    unsafe {vec.to_jsval( context, obj)};
}

#[cfg(test)]
mod tests {
    use crate::es_utils::tests::test_with_sm_rt;
    use crate::es_utils::{get_es_obj_prop_val, report_es_ex};
    use crate::es_utils::arrays::{get_array_length, object_is_array};

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

            let arr_val = get_es_obj_prop_val(context, global_root.handle(), "test_is_array");
            let arr_val2 = get_es_obj_prop_val(context, global_root.handle(), "test_is_array2");
            rooted!(in (context) let arr_val_root = arr_val.to_object());
            rooted!(in (context) let arr_val2_root = arr_val2.to_object());

            assert_eq!(true, object_is_array(context,  arr_val_root.handle()));

            assert_eq!(4, get_array_length(context, arr_val_root.handle()));

            assert_eq!(false, object_is_array(context,  arr_val2_root.handle()));

            true
        });

        assert_eq!(res, true);

    }
}