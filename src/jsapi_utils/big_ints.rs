use crate::jsapi_utils;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSType;
use mozjs::jsval::UndefinedValue;
use mozjs::rust::{HandleObject, HandleValue};

// fun fact.. bigint is NOT an object but JSVal does not have to_bigint.. just like it does not have to_function but functionvals to_object just fine

pub fn is_big_int(cx: *mut JSContext, val: HandleValue) -> bool {
    let js_type = jsapi_utils::get_type_of(cx, val);
    js_type == JSType::JSTYPE_BIGINT
}

pub fn as_string(cx: *mut JSContext, global: HandleObject, obj: HandleValue) -> String {
    rooted!(in (cx) let mut str_val = UndefinedValue());

    jsapi_utils::functions::call_namespace_function_name(
        cx,
        global,
        vec!["BigInt", "prototype", "toString"],
        "apply",
        vec![obj.get()],
        str_val.handle_mut(),
    )
    .ok()
    .expect("call_namespace_function_name failed");

    jsapi_utils::es_value_to_str(cx, str_val.handle().get())
        .ok()
        .expect("could not convert to string")
}

// todo
// get as u128 / etc
// new from string or u128
// compare two bigints
// compare with u128 etc

#[cfg(test)]
pub mod tests {
    use crate::jsapi_utils;
    use crate::jsapi_utils::big_ints::{as_string, is_big_int};
    use crate::jsapi_utils::tests::test_with_sm_rt;
    use mozjs::jsval::UndefinedValue;

    #[test]
    fn test_bigint() {
        test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in (cx) let mut rval = UndefinedValue());
                jsapi_utils::eval(
                    rt,
                    global,
                    "(BigInt(12345678901234567890n));",
                    "test_bigint.es",
                    rval.handle_mut(),
                )
                .ok()
                .expect("bigint script failed");

                assert!(is_big_int(cx, rval.handle()));

                assert_eq!(
                    as_string(cx, global, rval.handle()).as_str(),
                    "12345678901234567890"
                );
            });
        });
    }
}
