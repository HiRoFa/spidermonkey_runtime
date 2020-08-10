use log::trace;
use mozjs::conversions::{ConversionBehavior, FromJSValConvertible, ToJSValConvertible};
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::Int32Value;
use mozjs::rust::{HandleObject, HandleValue, MutableHandleObject, MutableHandleValue};
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

// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt8Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt16Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint8Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint16Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsFloat32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsFloat64Array.html

use crate::jsapi_utils::{get_pending_exception_or_generic_err, EsErrorInfo};
use mozjs::jsapi::JS_IsInt8Array;
use mozjs::jsapi::JS_NewInt8Array;

pub fn is_int8_array(obj: *mut JSObject) -> bool {
    unsafe { JS_IsInt8Array(obj) }
}

pub fn new_int8_array(cx: *mut JSContext, ret: MutableHandleObject, len: u32) {
    let mut ret = ret;
    ret.set(unsafe { JS_NewInt8Array(cx, len) });
}

pub fn new_int8_array_from_vec(
    cx: *mut JSContext,
    ret: MutableHandleObject,
    vec: Vec<i8>,
) -> Result<(), EsErrorInfo> {
    new_int8_array(cx, ret, vec.len() as u32);

    for (x, i) in vec.into_iter().enumerate() {
        rooted!(in (cx) let mut val_root = Int32Value(i.into()));
        let ok =
            unsafe { mozjs::jsapi::JS_SetElement3(cx, ret.handle().into(), x as u32, i as i32) };

        if !ok {
            return Err(get_pending_exception_or_generic_err(
                cx,
                "failed to fill new int8 array",
            ));
        }
    }
    Ok(())
}

pub fn int8_array_to_vec(_cx: *mut JSContext, arr: HandleObject) -> Result<Vec<i8>, EsErrorInfo> {
    let mut len: u32 = 0;
    let mut data = std::ptr::null_mut();
    let mut is_shared_mem = false;
    trace!("int8_array_to_vec / 1");
    unsafe {
        mozjs::glue::GetInt8ArrayLengthAndData(arr.get(), &mut len, &mut is_shared_mem, &mut data);
    };
    trace!("int8_array_to_vec / 2");
    let ulen = len as usize;
    // copy data first
    let mut vec = Vec::new();
    trace!("int8_array_to_vec / 3");
    vec.reserve(ulen);

    trace!("int8_array_to_vec / 4");
    unsafe {
        std::ptr::copy_nonoverlapping(data, vec.as_mut_ptr(), ulen);
        trace!("int8_array_to_vec / 5");
        vec.set_len(ulen);
    };

    trace!("int8_array_to_vec / 6");
    Ok(vec)
}

#[cfg(test)]
pub mod tests {
    use crate::jsapi_utils::arrays::{get_array_element, get_array_length, set_array_element};
    use crate::jsapi_utils::objects::NULL_JSOBJECT;
    use crate::jsapi_utils::typed_arrays::{
        int8_array_to_vec, is_int8_array, new_int8_array, new_int8_array_from_vec,
    };
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use log::trace;
    use mozjs::jsval::Int32Value;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_typed_array() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {
                rooted!(in (cx) let mut arr_obj_root = NULL_JSOBJECT);

                new_int8_array(cx, arr_obj_root.handle_mut(), 8);

                assert!(is_int8_array(arr_obj_root.handle().get()));

                for x in 0..8 {
                    rooted!(in (cx) let val_root = Int32Value((x * 3) as i32));
                    set_array_element(cx, arr_obj_root.handle(), x, val_root.handle())
                        .ok()
                        .expect("could not set array elem");
                }

                assert_eq!(
                    get_array_length(cx, arr_obj_root.handle())
                        .ok()
                        .expect("get len failed"),
                    8
                );

                rooted!(in (cx) let mut val4 = UndefinedValue());
                get_array_element(cx, arr_obj_root.handle(), 4, val4.handle_mut())
                    .ok()
                    .expect("get elem failed");
                assert!(val4.handle().get().is_int32());
                assert_eq!(val4.handle().get().to_int32(), 12);
            });
        });
    }

    #[test]
    fn test_typed_array_conversion() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {
                let vec: Vec<i8> = vec![4, 8, 2, 105];

                rooted!(in (cx) let mut arr_obj_root = NULL_JSOBJECT);

                new_int8_array_from_vec(cx, arr_obj_root.handle_mut(), vec)
                    .ok()
                    .expect("new_int8_array_from_vec failed");

                assert!(is_int8_array(arr_obj_root.get()));

                let converted_vec = int8_array_to_vec(cx, arr_obj_root.handle())
                    .ok()
                    .expect("could not convert to vec");

                trace!("test_typed_array_conversion / 1");

                assert_eq!(converted_vec.len(), 4);
                trace!("test_typed_array_conversion / 2");

                assert_eq!(converted_vec.get(0).unwrap(), &4);

                trace!("test_typed_array_conversion / 3");
                assert_eq!(converted_vec.get(3).unwrap(), &105);
                trace!("test_typed_array_conversion / 4");
            });
        });
    }
}
