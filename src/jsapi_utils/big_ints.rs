use crate::jsapi_utils;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSType;
use mozjs::rust::HandleValue;

pub fn is_big_int(cx: *mut JSContext, val: HandleValue) -> bool {
    let js_type = jsapi_utils::get_type_of(cx, val);
    js_type == JSType::JSTYPE_BIGINT
}

// todo, get as string
// get as u128 / etc
// new from string or u128
// compare two bigints
// compare with u128 etc

#[cfg(test)]
pub mod tests {
    use crate::jsapi_utils;
    use crate::jsapi_utils::big_ints::is_big_int;
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
                    "(BigInt(12345678901234567890));",
                    "test_bigint.es",
                    rval.handle_mut(),
                )
                .ok()
                .expect("bigint script failed");

                assert!(is_big_int(cx, rval.handle()));
            });
        });
    }
}
