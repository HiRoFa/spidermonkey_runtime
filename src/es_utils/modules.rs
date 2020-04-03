use crate::es_utils::{report_es_ex, EsErrorInfo};

use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use std::ffi::CString;
use std::ptr;

pub fn compile_module(
    context: *mut JSContext,
    src: &str,
    file_name: &str,
) -> Result<*mut JSObject, EsErrorInfo> {
    // use mozjs::jsapi::CompileModule; todo, how are the wrapped once different?
    // https://doc.servo.org/mozjs/jsapi/fn.CompileModule.html

    rooted!(in(context) let mut module_script_root = ptr::null_mut::<mozjs::jsapi::JSObject>());

    let src_vec: Vec<u16> = src.encode_utf16().collect();
    let file_name_cstr = CString::new(file_name).unwrap();
    let options = mozjs::rust::CompileOptionsWrapper::new(context, file_name_cstr.as_ptr(), 1);
    let mut source = mozjs::jsapi::SourceBufferHolder {
        data_: src_vec.as_ptr(),
        length_: src_vec.len() as libc::size_t,
        ownsChars_: false,
    };

    let res = unsafe {
        mozjs::rust::wrappers::CompileModule(
            context,
            options.ptr,
            &mut source,
            module_script_root.handle_mut(),
        )
    };

    if !res {
        if let Some(err) = report_es_ex(context) {
            return Err(err);
        }
    }

    unsafe { mozjs::rust::wrappers::ModuleInstantiate(context, module_script_root.handle()) };
    if let Some(err) = report_es_ex(context) {
        return Err(err);
    }

    unsafe { mozjs::rust::wrappers::ModuleEvaluate(context, module_script_root.handle()) };
    if let Some(err) = report_es_ex(context) {
        return Err(err);
    }

    Ok(*module_script_root)
}

#[cfg(test)]
mod tests {

    use crate::es_utils::modules::compile_module;
    use crate::es_utils::tests::test_with_sm_rt;

    #[test]
    fn test_module() {
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
}
