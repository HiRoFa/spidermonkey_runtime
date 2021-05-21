// compilescript and execute script

// JS_Compile
// JS_ExecuteScript

// useful for compiling stuff like async function and then running it

use crate::jsapi_utils;
use crate::jsapi_utils::EsErrorInfo;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSScript;

use mozjs::rust::{
    transform_u16_to_source_text, HandleScript, MutableHandleScript, MutableHandleValue,
};

/// compile a script, return a JSScript object via rval which can be executed by calling execute_script
/// ```no_run
/// use spidermonkey_runtime::jsapi_utils::scripts::{execute_script, compile_script};
/// use spidermonkey_runtime::jsapi_utils;
/// use mozjs::rooted;
/// use mozjs::jsapi::JSScript;
/// use mozjs::jsval::UndefinedValue;
/// use std::ptr;
/// use spidermonkey_runtime::esruntimebuilder::EsRuntimeBuilder;
/// let rt = EsRuntimeBuilder::new().build();
/// rt.do_in_es_event_queue_sync(|sm_rt| {
///     sm_rt.do_with_jsapi(|_rt, cx, _global| {
///         let script = "(async function foo() {return 123;})();";
///         rooted!(in (cx) let mut script_res = ptr::null_mut::<JSScript>());
///         let compile_res =
///             compile_script(cx, script, "test_scripts.es", script_res.handle_mut());
///         if let Some(err) = compile_res.err() {
///             panic!("could not compile script: {}", err.err_msg());
///         }
///         rooted!(in (cx) let mut script_exe_res = UndefinedValue());
///         let exe_res = execute_script(cx, script_res.handle(), script_exe_res.handle_mut());
///         if let Some(err) = exe_res.err() {
///             panic!("script exe failed: {}", err.err_msg());
///         }
///         assert!(jsapi_utils::promises::value_is_promise(
///             script_exe_res.handle()
///         ));
///     });
/// });
/// ```
pub fn compile_script(
    cx: *mut JSContext,
    src: &str,
    file_name: &str,
    rval: MutableHandleScript,
) -> Result<(), EsErrorInfo> {
    let src_vec: Vec<u16> = src.encode_utf16().collect();
    let options = unsafe { mozjs::rust::CompileOptionsWrapper::new(cx, file_name, 1) };
    let mut source = transform_u16_to_source_text(&src_vec);

    let compiled_script: *mut JSScript =
        unsafe { mozjs::jsapi::Compile(cx, options.ptr, &mut source) };

    rooted!(in (cx) let compiled_script_root = compiled_script);

    if compiled_script_root.is_null() {
        let err_opt = jsapi_utils::get_pending_exception(cx);
        if let Some(err) = err_opt {
            return Err(err);
        }
    }

    let mut rval = rval;
    rval.set(compiled_script);
    Ok(())
}

/// execute a compiled script
pub fn execute_script(
    cx: *mut JSContext,
    script: HandleScript,
    rval: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let ok = unsafe { mozjs::jsapi::JS_ExecuteScript(cx, script.into(), rval.into()) };
    if !ok {
        let err_opt = jsapi_utils::get_pending_exception(cx);
        return if let Some(err) = err_opt {
            Err(err)
        } else {
            Err(EsErrorInfo {
                message: "unknown error while executing script occured".to_string(),
                filename: "execute_script".to_string(),
                lineno: 0,
                column: 0,
            })
        };
    }

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use crate::esruntime::tests::init_test_runtime;
    use crate::jsapi_utils;
    use crate::jsapi_utils::scripts::{compile_script, execute_script};
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use log::debug;
    use mozjs::jsapi::JSScript;
    use mozjs::jsval::UndefinedValue;
    use std::ptr;
    use std::time::Duration;

    #[test]
    fn test_scripts() {
        log::info!("test_scripts");
        std::thread::sleep(Duration::from_secs(1));

        let rt = init_test_runtime();

        rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
            sm_rt.do_with_jsapi(|_rt, cx, _global| {
                //
                let script = "(async function foo() {return 123;})();";
                debug!("about to compile script {}", script);

                rooted!(in (cx) let mut script_res = ptr::null_mut::<JSScript>());
                let compile_res =
                    compile_script(cx, script, "test_scripts.es", script_res.handle_mut());
                if let Some(err) = compile_res.err() {
                    panic!("could not compile script: {}", err.err_msg());
                }

                debug!("about to exe script {}", script);

                rooted!(in (cx) let mut script_exe_res = UndefinedValue());
                let exe_res = execute_script(cx, script_res.handle(), script_exe_res.handle_mut());
                if let Some(err) = exe_res.err() {
                    panic!("script exe failed: {}", err.err_msg());
                }

                // i expect a promise

                assert!(jsapi_utils::promises::value_is_promise(
                    script_exe_res.handle()
                ));
            });
        });
    }
}
