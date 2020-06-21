use crate::jsapi_utils::{report_es_ex, EsErrorInfo};

use log::trace;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::rust::transform_u16_to_source_text;
use std::ffi::CString;

// compile a module script, this does not cache the module, use SmRuntime::load_module for that
pub fn compile_module(
    context: *mut JSContext,
    src: &str,
    file_name: &str,
) -> Result<*mut JSObject, EsErrorInfo> {
    // use mozjs::jsapi::CompileModule; todo, how are the wrapped ones different?
    // https://doc.servo.org/mozjs/jsapi/fn.CompileModule.html

    trace!("compile_module: {}", file_name);
    trace!("{}", src);

    let src_vec: Vec<u16> = src.encode_utf16().collect();
    let file_name_cstr = CString::new(file_name).unwrap();
    let options =
        unsafe { mozjs::rust::CompileOptionsWrapper::new(context, file_name_cstr.as_ptr(), 1) };
    let mut source = transform_u16_to_source_text(&src_vec);

    let compiled_module: *mut JSObject =
        unsafe { mozjs::jsapi::CompileModule(context, options.ptr, &mut source) };

    rooted!(in(context) let mut module_script_root = compiled_module);

    // see ModuleInstantiate
    if module_script_root.is_null() {
        // failed
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
        return Err(EsErrorInfo {
            message: "CompileModule failed unknown".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        });
    }

    trace!("ModuleInstantiate: {}", file_name);

    let res =
        unsafe { mozjs::rust::wrappers::ModuleInstantiate(context, module_script_root.handle()) };
    if !res {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
        return Err(EsErrorInfo {
            message: "ModuleInstantiate failed unknown".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        });
    }

    trace!("ModuleEvaluate: {}", file_name);

    let res =
        unsafe { mozjs::rust::wrappers::ModuleEvaluate(context, module_script_root.handle()) };
    if !res {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
        return Err(EsErrorInfo {
            message: "ModuleEvaluate failed unknown".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        });
    }

    Ok(compiled_module)
}

#[cfg(test)]
mod tests {

    use crate::jsapi_utils::modules::compile_module;
    use crate::jsapi_utils::tests::test_with_sm_rt;
    use std::time::Duration;

    #[test]
    fn test_module() {
        log::info!("test: test_module");
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {

            let mod_script =
                "export default () => 123; let myPrivate = 12; \n\nconsole.log('running a module %i', myPrivate);";

            let compile_res = compile_module(cx, mod_script, "test_mod.es");
            if compile_res.is_err() {
                let err = compile_res.err().unwrap();
                panic!(
                    "error compiling module: {}:{}:{} err:{}",
                    err.filename, err.lineno, err.column, err.message
                );
            }

            let mod_script2 =
                "import {other} from 'test_mod.es';\n\nconsole.log('started mod imp mod, other = ' + other);";

            let compile_res2 = compile_module(cx, mod_script2, "test_mod2.es");
            if compile_res2.is_err() {
                let err = compile_res2.err().unwrap();
                panic!(
                    "error compiling module: {}:{}:{} err:{}",
                    err.filename, err.lineno, err.column, err.message
                );
            }

            true
            })
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_module_gc() {
        log::info!("test: test_module_gc");
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {

                let mod_script2 =
                    "import {other} from 'test_mod.es';\n\nconsole.log('started mod imp mod, other = ' + other);";

                let compile_res2 = compile_module(cx, mod_script2, "test_mod2.es");
                if compile_res2.is_err() {
                    let err = compile_res2.err().unwrap();
                    panic!(
                        "error compiling module: {}:{}:{} err:{}",
                        err.filename, err.lineno, err.column, err.message
                    );
                }


            });
            sm_rt.cleanup();
            sm_rt.do_with_jsapi(|_rt, cx, _global| {

                let mod_script2 =
                    "import {other} from 'test_mod.es';\n\nconsole.log('started mod imp mod, other = ' + other);";

                let compile_res2 = compile_module(cx, mod_script2, "test_mod2.es");
                if compile_res2.is_err() {
                    let err = compile_res2.err().unwrap();
                    panic!(
                        "error compiling module: {}:{}:{} err:{}",
                        err.filename, err.lineno, err.column, err.message
                    );
                }


            });

            true
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_dynamic_import() {
        log::info!("test: test_dynamic_import");
        let prom_esvf = test_with_sm_rt(|sm_rt| {
            let eval_res = sm_rt.eval(
                "let test_dynamic_import_mod_prom = import('foo_test_mod_dyn.mes').then((res) => {return ('ok' + res.other);})\
                              .catch((pex) => {return ('err' + pex);});\
                              test_dynamic_import_mod_prom;",
                "test_dynamic_import.es",
            );

            match eval_res {
                Ok(ok_esvf) => {
                    assert!(ok_esvf.is_promise());
                    ok_esvf
                }
                Err(err) => panic!("script failed 2: {}", err.err_msg()),
            }
        });

        let prom_res = prom_esvf
            .get_promise_result_blocking(Duration::from_secs(60))
            .expect("promise timed out");
        match prom_res {
            Ok(s) => {
                assert!(s.is_string());
                assert!(s.get_string().starts_with("ok"))
            }
            Err(err) => panic!("script failed 1: {}", err.get_string()),
        }

        log::debug!("test_dynamic_import: import should be done now");
        // 'foo_test_mod.mes'
    }
    #[test]
    // see if 404 module works as expected
    fn test_dynamic_import2() {
        log::info!("test: test_dynamic_import2");
        let prom_esvf = test_with_sm_rt(|sm_rt| {
            let eval_res = sm_rt.eval(
                "let test_dynamic_import_mod2_prom = import('dontfind.mes').then((res) => {return ('ok' + res.other);})\
                              .catch((pex) => {return ('err' + pex);});\
                              test_dynamic_import_mod2_prom;",
                "test_dynamic_import.es",
            );

            match eval_res {
                Ok(ok_esvf) => {
                    assert!(ok_esvf.is_promise());
                    ok_esvf
                }
                Err(err) => panic!("script failed 2: {}", err.err_msg()),
            }
        });

        let prom_res = prom_esvf
            .get_promise_result_blocking(Duration::from_secs(60))
            .expect("promise timed out");
        match prom_res {
            Ok(s) => {
                assert!(s.is_string());
                assert!(s.get_string().starts_with("err"))
            }
            Err(err) => panic!("script failed 1: {}", err.get_string()),
        }

        log::debug!("test_dynamic_import2: import should be done now");
        // 'foo_test_mod.mes'
    }
    #[test]
    // see if compile fail works as expected
    fn test_dynamic_import3() {
        log::info!("test: test_dynamic_import2");
        let prom_esvf = test_with_sm_rt(|sm_rt| {
            let eval_res = sm_rt.eval(
                "let test_dynamic_import_mod3_prom = import('buggy.mes').then((res) => {return ('ok' + res.other);})\
                              .catch((pex) => {return ('err' + pex);});\
                              test_dynamic_import_mod3_prom;",
                "test_dynamic_import.es",
            );

            match eval_res {
                Ok(ok_esvf) => {
                    assert!(ok_esvf.is_promise());
                    ok_esvf
                }
                Err(err) => panic!("script failed 2: {}", err.err_msg()),
            }
        });

        let prom_res = prom_esvf
            .get_promise_result_blocking(Duration::from_secs(60))
            .expect("promise timed out");
        match prom_res {
            Ok(s) => {
                assert!(s.is_string());
                assert!(s.get_string().starts_with("err"))
            }
            Err(err) => panic!("script failed 1: {}", err.get_string()),
        }

        log::debug!("test_dynamic_import2: import should be done now");
        // 'foo_test_mod.mes'
    }
    #[test]
    // run again, see if cached works
    fn test_dynamic_import4() {
        log::info!("test: test_dynamic_import");
        let prom_esvf = test_with_sm_rt(|sm_rt| {
            let eval_res = sm_rt.eval(
                "let test_dynamic_import_mod4_prom = import('foo_test_mod_dyn.mes').then((res) => {return ('ok' + res.other);})\
                              .catch((pex) => {return ('err' + pex);});\
                              test_dynamic_import_mod4_prom;",
                "test_dynamic_import.es",
            );

            match eval_res {
                Ok(ok_esvf) => {
                    assert!(ok_esvf.is_promise());
                    ok_esvf
                }
                Err(err) => panic!("script failed 2: {}", err.err_msg()),
            }
        });

        let prom_res = prom_esvf
            .get_promise_result_blocking(Duration::from_secs(60))
            .expect("promise timed out");
        match prom_res {
            Ok(s) => {
                assert!(s.is_string());
                assert!(s.get_string().starts_with("ok"))
            }
            Err(err) => panic!("script failed 1: {}", err.get_string()),
        }

        log::debug!("test_dynamic_import: import should be done now");
        // 'foo_test_mod.mes'
    }
}
