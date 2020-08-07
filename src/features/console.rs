use crate::esruntime::EsRuntime;
use crate::jsapi_utils;
use crate::jsapi_utils::reflection::ProxyBuilder;
use crate::jsapi_utils::report_exception;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use mozjs::jsapi::CallArgs;
use mozjs::jsapi::JSContext;
use mozjs::jsval::{JSVal, UndefinedValue};
use mozjs::rust::HandleValue;
use std::str::FromStr;

// todo rewrite to Proxy

pub(crate) fn init(rt: &EsRuntime) {
    rt.do_in_es_event_queue_sync(Box::new(|sm_rt: &SmRuntime| {
        sm_rt.do_with_jsapi(|_rt, context, global| {
            ProxyBuilder::new(vec![], "console")
                .static_native_method("log", Some(console_log))
                .static_native_method("trace", Some(console_trace))
                .static_native_method("info", Some(console_info))
                .static_native_method("warn", Some(console_warn))
                .static_native_method("error", Some(console_error))
                .static_native_method("assert", Some(console_assert))
                .static_native_method("debug", Some(console_debug))
                .build(context, global);
        });
    }));
}

///
/// this method parses a field code in the form of %s or %.1d
/// see https://console.spec.whatwg.org/#formatting-specifiers
///
fn parse_field(context: *mut JSContext, field: String, value: JSVal) -> String {
    rooted!(in(context) let val_root = value);

    // convert all vartypes to jsstring
    let js_str: *mut mozjs::jsapi::JSString =
        unsafe { mozjs::rust::ToString(context, val_root.handle()) };
    // convert jsstring to rust string
    let str_val = jsapi_utils::es_jsstring_to_string(context, js_str);

    // return string
    parse_field_value(field, str_val)
}

fn parse_field_value(field: String, value: String) -> String {
    // format ints
    // only support ,2 / .3 to declare the number of digits to display, e.g. $.3i turns 3 to 003

    // format floats
    // only support ,2 / .3 to declare the number of decimals to display, e.g. $.3f turns 3.1 to 3.100

    if field.eq(&"%.0f".to_string()) {
        return parse_field_value("%i".to_string(), value);
    }

    if field.ends_with('d') || field.ends_with('i') {
        let mut i_val = value;

        // remove chars behind .
        if let Some(i) = i_val.find('.') {
            let _ = i_val.split_off(i);
        }

        if let Some(dot_in_field_idx) = field.find('.') {
            let mut m_field = field.clone();
            // get part behind dot
            let mut num_decimals_str = m_field.split_off(dot_in_field_idx + 1);
            // remove d or i at end
            let _ = num_decimals_str.split_off(num_decimals_str.len() - 1);
            // see if we have a number
            if !num_decimals_str.is_empty() {
                let ct_res = usize::from_str(num_decimals_str.as_str());
                // check if we can parse the number to a usize
                if let Ok(ct) = ct_res {
                    // and if so, make i_val longer
                    while i_val.len() < ct {
                        i_val = format!("0{}", i_val);
                    }
                }
            }
        }

        return i_val;
    } else if field.ends_with('f') {
        let mut f_val = value;

        if let Some(dot_in_field_idx) = field.find('.') {
            let mut m_field = field.clone();
            // get part behind dot
            let mut num_decimals_str = m_field.split_off(dot_in_field_idx + 1);
            // remove d or i at end
            let _ = num_decimals_str.split_off(num_decimals_str.len() - 1);
            // see if we have a number
            if !num_decimals_str.is_empty() {
                let ct_res = usize::from_str(num_decimals_str.as_str());
                // check if we can parse the number to a usize
                if let Ok(ct) = ct_res {
                    // and if so, make i_val longer
                    if ct > 0 {
                        if !f_val.contains('.') {
                            f_val.push('.');
                        }

                        let dot_idx = f_val.find('.').unwrap();

                        while f_val.len() - dot_idx <= ct {
                            f_val.push('0');
                        }
                        if f_val.len() - dot_idx > ct {
                            let _ = f_val.split_off(dot_idx + ct + 1);
                        }
                    }
                }
            }
        }

        return f_val;
    }
    value
}

// todo add an extra format_field_value method which does NOT require a context so we can unit test that independantly

fn parse_line(context: *mut JSContext, argc: u32, vp: *mut mozjs::jsapi::Value) -> String {
    let args = unsafe { CallArgs::from_vp(vp, argc) };

    let mut values: Vec<JSVal> = vec![];
    for x in 0..args.argc_ {
        let argx: HandleValue = unsafe { mozjs::rust::Handle::from_raw(args.get(x)) };
        let argx_val: mozjs::jsapi::Value = *argx;
        values.push(argx_val);
    }

    args.rval().set(UndefinedValue());

    parse_line2(context, values)
}

fn parse_line2(context: *mut JSContext, args: Vec<JSVal>) -> String {
    if args.is_empty() {
        return "".to_string();
    }
    let mut args = args;
    let arg1: JSVal = args.remove(0);
    let message = jsapi_utils::es_value_to_str(context, arg1).ok().unwrap();

    let mut output = String::new();
    let mut field_code = String::new();
    let mut in_field = false;

    for chr in message.chars() {
        if in_field {
            field_code.push(chr);
            if chr.eq(&'s') || chr.eq(&'d') || chr.eq(&'f') || chr.eq(&'o') || chr.eq(&'i') {
                // end field
                if !args.is_empty() {
                    output.push_str(parse_field(context, field_code, args.remove(0)).as_str());
                }

                in_field = false;
                field_code = String::new();
            }
        } else if chr.eq(&'%') {
            in_field = true;
        } else {
            output.push(chr);
        }
    }

    output
}

unsafe extern "C" fn console_log(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::info!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_debug(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::debug!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_warn(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::warn!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_info(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::info!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_trace(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::trace!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_error(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    //
    log::error!("console: {}", parse_line(context, argc, vp));
    true
}

unsafe extern "C" fn console_assert(
    context: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    if argc < 2 {
        report_exception(context, "console.assert requires at least 2 arguments");
    }

    let mut values: Vec<JSVal> = vec![];
    for x in 0..args.argc_ {
        let argx: HandleValue = mozjs::rust::Handle::from_raw(args.get(x));
        let argx_val: mozjs::jsapi::Value = *argx;
        values.push(argx_val);
    }

    let assertion_val = values.remove(0);
    if !assertion_val.is_boolean() {
        report_exception(
            context,
            "first argument to console.assert should be a boolean value",
        );
    }
    let assertion: bool = assertion_val.to_boolean();

    args.rval().set(UndefinedValue());

    if assertion {
        log::info!("console: {}", parse_line2(context, values));
    }

    true
}

#[cfg(test)]
mod tests {
    use crate::esvaluefacade::EsValueFacade;
    use crate::features::console::parse_field_value;

    #[test]
    fn test_patterns() {
        assert_eq!("1", parse_field_value("%i".to_string(), "1".to_string()));
        assert_eq!("1", parse_field_value("%i".to_string(), "1.1".to_string()));
        assert_eq!("01", parse_field_value("%.2i".to_string(), "1".to_string()));
        assert_eq!(
            "01",
            parse_field_value("%.2i".to_string(), "1.1".to_string())
        );
    }

    #[test]
    fn test_f_patterns() {
        assert_eq!("1", parse_field_value("%.0f".to_string(), "1".to_string()));
        assert_eq!(
            "1.0",
            parse_field_value("%.1f".to_string(), "1".to_string())
        );
        assert_eq!(
            "1",
            parse_field_value("%.0f".to_string(), "1.1".to_string())
        );
        assert_eq!(
            "1.1",
            parse_field_value("%.1f".to_string(), "1.1".to_string())
        );
        assert_eq!(
            "1.10",
            parse_field_value("%.2f".to_string(), "1.1".to_string())
        );
        assert_eq!(
            "1.100",
            parse_field_value("%.3f".to_string(), "1.1".to_string())
        );
        assert_eq!(
            "1.000",
            parse_field_value("%.3f".to_string(), "1".to_string())
        );
        assert_eq!(
            "1.000",
            parse_field_value("%.3f".to_string(), "1.000000".to_string())
        );
    }

    #[test]
    fn test_console() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        //let console: EsValueFacade = rt.eval_sync("(console);", "test_console.es").ok().unwrap();

        // assert!(console.is_object());
        // see todo in proxy, obj instead of function if no constructor

        // not realy a test, just check output yourself
        rt.eval_sync("let c = console;c.log('test log');c.info('test info %s %.2d %.2f', 'strval1', 1.1, 12);c.error('test error');c.warn('test warn');c.debug('test debug');c.trace('test trace');", "test_console.es")
            .ok()
            .unwrap();
    }
}
