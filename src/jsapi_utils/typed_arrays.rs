use log::trace;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::rust::{HandleObject, MutableHandleObject};

// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt8Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt16Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsInt32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint8Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint16Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsUint32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsFloat32Array.html
// https://doc.servo.org/mozjs/jsapi/fn.JS_IsFloat64Array.html

// So after creating this i found the TypedArray struct in mozjs::typedarray::TypedArray which pretty much does the same
// but hey, it was good practice and it's nice to see i came to pretty much the same solution for a problem

use crate::jsapi_utils::arrays;
use crate::jsapi_utils::EsErrorInfo;

macro_rules! typed_array {
    (
        $struct_ident:ident,
        $is_method:ident,
        $new_method:ident,
        $length_and_data_method:ident,
        $set_element_method:ident,
        $jsval_to_val_method:ident,
        $rust_type:ty,
        $es_rust_type:ty
    ) => {
        pub struct $struct_ident {}

        impl $struct_ident {
            pub fn is_instance(obj: *mut JSObject) -> bool {
                unsafe { mozjs::jsapi::$is_method(obj) }
            }
            pub fn new_instance(cx: *mut JSContext, ret: MutableHandleObject, len: usize) {
                let mut ret = ret;
                ret.set(unsafe { mozjs::jsapi::$new_method(cx, len) });
            }
            pub fn set_element(
                cx: *mut JSContext,
                arr: HandleObject,
                index: u32,
                val: $rust_type,
            ) -> Result<(), EsErrorInfo> {
                arrays::$set_element_method(cx, arr, index, val as $es_rust_type)
            }
            pub fn get_element(
                cx: *mut JSContext,
                arr_obj: HandleObject,
                idx: u32,
            ) -> Result<$rust_type, EsErrorInfo> {
                rooted!(in (cx) let mut rval = mozjs::jsval::UndefinedValue());
                let get_res = arrays::get_array_element(cx, arr_obj, idx, rval.handle_mut());
                match get_res {
                    Ok(_) => {
                        Ok(rval.$jsval_to_val_method() as $rust_type)
                    },
                    Err(err) => {
                        Err(err)
                    }
                }
            }
            pub fn new_instance_from_vec(
                cx: *mut JSContext,
                ret: MutableHandleObject,
                vec: Vec<$rust_type>,
            ) -> Result<(), EsErrorInfo> {
                trace!("new_typed_array_from_vec / 1");

                $struct_ident::new_instance(cx, ret, vec.len());

                let mut len: usize = 0;
                let mut data = std::ptr::null_mut();
                let mut is_shared_mem = false;
                trace!("new_typed_array_from_vec / 2");
                unsafe {
                    mozjs::glue::$length_and_data_method(
                        ret.get(),
                        &mut len,
                        &mut is_shared_mem,
                        &mut data,
                    );
                };
                trace!("new_typed_array_from_vec / 3");
                let ulen = len as usize;

                unsafe {
                    std::ptr::copy_nonoverlapping(vec.as_ptr(), data, ulen);
                };
                trace!("new_typed_array_from_vec / 4");

                Ok(())
            }
            pub fn convert_to_vec(
                _cx: *mut JSContext,
                arr: HandleObject,
            ) -> Result<Vec<$rust_type>, EsErrorInfo> {
                let mut len: usize = 0;
                let mut data = std::ptr::null_mut();
                let mut is_shared_mem = false;
                trace!("to_vec / 1");
                unsafe {
                    mozjs::glue::$length_and_data_method(
                        arr.get(),
                        &mut len,
                        &mut is_shared_mem,
                        &mut data,
                    );
                };
                trace!("to_vec / 2");
                let ulen = len as usize;
                // copy data first
                let mut vec = Vec::new();
                trace!("to_vec / 3");
                vec.reserve(ulen);

                trace!("to_vec / 4");
                unsafe {
                    std::ptr::copy_nonoverlapping(data, vec.as_mut_ptr(), ulen);
                    trace!("to_vec / 5");
                    vec.set_len(ulen);
                };

                trace!("to_vec / 6");
                Ok(vec)
            }
        }
    };
}

typed_array!(
    Int8Array,
    JS_IsInt8Array,
    JS_NewInt8Array,
    GetInt8ArrayLengthAndData,
    set_array_element_i32,
    to_int32,
    i8,
    i32
);
typed_array!(
    Uint8Array,
    JS_IsUint8Array,
    JS_NewUint8Array,
    GetUint8ArrayLengthAndData,
    set_array_element_u32,
    to_int32,
    u8,
    u32
);
typed_array!(
    Int16Array,
    JS_IsInt16Array,
    JS_NewInt16Array,
    GetInt16ArrayLengthAndData,
    set_array_element_i32,
    to_int32,
    i16,
    i32
);
typed_array!(
    Uint16Array,
    JS_IsUint16Array,
    JS_NewUint16Array,
    GetUint16ArrayLengthAndData,
    set_array_element_u32,
    to_int32,
    u16,
    u32
);
typed_array!(
    Int32Array,
    JS_IsInt32Array,
    JS_NewInt32Array,
    GetInt32ArrayLengthAndData,
    set_array_element_i32,
    to_int32,
    i32,
    i32
);
typed_array!(
    Uint32Array,
    JS_IsUint32Array,
    JS_NewUint32Array,
    GetUint32ArrayLengthAndData,
    set_array_element_u32,
    to_int32,
    u32,
    u32
);
typed_array!(
    Float32Array,
    JS_IsFloat32Array,
    JS_NewFloat32Array,
    GetFloat32ArrayLengthAndData,
    set_array_element_f64,
    to_number,
    f32,
    f64
);
typed_array!(
    Float64Array,
    JS_IsFloat64Array,
    JS_NewFloat64Array,
    GetFloat64ArrayLengthAndData,
    set_array_element_f64,
    to_number,
    f64,
    f64
);

#[cfg(test)]
pub mod tests {
    use crate::jsapi_utils::arrays::{get_array_element, get_array_length, set_array_element_i32};
    use crate::jsapi_utils::objects::NULL_JSOBJECT;
    use crate::jsapi_utils::typed_arrays::Int8Array;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use log::trace;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_typed_array() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {
                rooted!(in (cx) let mut arr_obj_root = NULL_JSOBJECT);

                Int8Array::new_instance(cx, arr_obj_root.handle_mut(), 8);

                assert!(Int8Array::is_instance(arr_obj_root.handle().get()));

                for x in 0..8 {
                    set_array_element_i32(cx, arr_obj_root.handle(), x, (x * 3) as i32)
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

                Int8Array::new_instance_from_vec(cx, arr_obj_root.handle_mut(), vec)
                    .ok()
                    .expect("new_int8_array_from_vec failed");

                assert!(Int8Array::is_instance(arr_obj_root.get()));

                let converted_vec = Int8Array::convert_to_vec(cx, arr_obj_root.handle())
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
