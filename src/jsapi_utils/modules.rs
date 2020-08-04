use crate::esruntime::{EsRuntime, EsScriptCode};
use crate::jsapi_utils;
use crate::jsapi_utils::rooting::EsPersistentRooted;
use crate::jsapi_utils::{get_pending_exception, report_exception2, EsErrorInfo};
use crate::spidermonkeyruntimewrapper::{register_cached_object, SmRuntime, SM_RT};
use log::trace;
use lru::LruCache;
use mozjs::jsapi::FinishDynamicModuleImport;
use mozjs::jsapi::Handle as RawHandle;
use mozjs::jsapi::HandleObject as RawHandleObject;
use mozjs::jsapi::HandleValue as RawHandleValue;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSString;
use mozjs::jsapi::SetModuleDynamicImportHook;
use mozjs::jsapi::SetModuleMetadataHook;
use mozjs::jsapi::SetModulePrivate;
use mozjs::jsapi::SetModuleResolveHook;
use mozjs::jsval::UndefinedValue;
use mozjs::jsval::{NullValue, ObjectValue, StringValue};
use mozjs::rust::{transform_u16_to_source_text, Runtime};
use std::cell::RefCell;
use std::ffi::CString;
use std::ptr;

/// prepare a Runtime for working with modules
/// this initializes the methods needed to load modules from script
/// but only for a runtime initialized from a EsRuntime so private for now
/// we use the EsRuntimes helper threadpool and the code loader
/// we should be able to pass those abstracted to this utl so it can be used as independent util
pub(crate) fn init_runtime_for_modules(rt: &Runtime) {
    unsafe {
        SetModuleResolveHook(rt.rt(), Some(import_module));
        SetModuleDynamicImportHook(rt.rt(), Some(module_dynamic_import));
        SetModuleMetadataHook(rt.rt(), Some(set_module_metadata));
    };
}

/// compile a module script, this does not cache the module, use SmRuntime::load_module for that
/// it runs CompileModule, ModuleInstantiate and ModuleEvaluate and returns an EsErrorInfo if any one fails
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
        if let Some(err) = get_pending_exception(context) {
            return Err(err);
        }
        return Err(EsErrorInfo {
            message: "CompileModule failed unknown".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        });
    }

    trace!("SetModulePrivate: {}", file_name);

    let private_obj = jsapi_utils::objects::new_object(context);
    rooted!(in (context) let private_obj_root = private_obj);
    rooted!(in (context) let mut path_root = UndefinedValue());
    jsapi_utils::new_es_value_from_str(context, file_name, path_root.handle_mut());

    jsapi_utils::objects::set_es_obj_prop_value(
        context,
        private_obj_root.handle(),
        "path",
        path_root.handle(),
    );
    unsafe { SetModulePrivate(compiled_module, &ObjectValue(private_obj)) };

    trace!("ModuleInstantiate: {}", file_name);

    let res =
        unsafe { mozjs::rust::wrappers::ModuleInstantiate(context, module_script_root.handle()) };
    if !res {
        if let Some(err) = get_pending_exception(context) {
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
        if let Some(err) = get_pending_exception(context) {
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

thread_local! {
// store epr in Box because https://doc.servo.org/mozjs_sys/jsgc/struct.Heap.html#method.boxed
    static MODULE_CACHE: RefCell<LruCache<String, EsPersistentRooted>> = RefCell::new(init_module_cache());
}

/// this initializes the LryCache based on your settings
/// i'm not sure yet if this is the way to go, i'm tempted to believe the engine keeps it's own module registry
fn init_module_cache() -> LruCache<String, EsPersistentRooted> {
    let ct = SM_RT.with(|sm_rt_rc| {
        let sm_rt = &*sm_rt_rc.borrow();
        sm_rt.clone_esrt_inner().module_cache_size
    });

    LruCache::new(ct)
}

fn get_path_from_module_private(cx: *mut JSContext, reference_private: RawHandleValue) -> String {
    if !reference_private.is_undefined() {
        rooted!(in (cx) let private_obj_root = reference_private.to_object_or_null());
        let path_res = jsapi_utils::objects::get_es_obj_prop_val_as_string(
            cx,
            private_obj_root.handle(),
            "path",
        );
        if path_res.is_ok() {
            return path_res.ok().unwrap();
        }
    }

    "(unknown)".to_string()
}

/// native function used for dynamic imports
unsafe extern "C" fn module_dynamic_import(
    cx: *mut JSContext,
    reference_private: RawHandleValue,
    specifier: RawHandle<*mut JSString>,
    promise: RawHandle<*mut JSObject>,
) -> bool {
    // see sequence here in c
    // https://github.com/mozilla/gecko-dev/blob/master/js/src/shell/ModuleLoader.cpp

    trace!("module_dynamic_import called");

    rooted!(in (cx) let mut closure_root = jsapi_utils::objects::new_object(cx));
    rooted!(in (cx) let promise_val_root = ObjectValue(*promise));
    rooted!(in (cx) let specifier_val_root = StringValue(&**specifier));
    jsapi_utils::objects::set_es_obj_prop_value(
        cx,
        closure_root.handle(),
        "promise",
        promise_val_root.handle(),
    );
    jsapi_utils::objects::set_es_obj_prop_value_raw(
        cx,
        closure_root.handle().into(),
        "reference_private",
        reference_private,
    );
    jsapi_utils::objects::set_es_obj_prop_value(
        cx,
        closure_root.handle(),
        "specifier",
        specifier_val_root.handle(),
    );

    let file_name = jsapi_utils::es_jsstring_to_string(cx, *specifier);
    let ref_path = get_path_from_module_private(cx, reference_private);

    trace!(
        "module_dynamic_import called: {} from ref: {}",
        file_name,
        ref_path
    );

    let closure_id = register_cached_object(cx, *closure_root);
    let rt_arc = SmRuntime::clone_current_esrt_inner_arc();

    // todo if the module is already cache we could just run an async job via
    // rt_arc.do_in_es_runtime_thread
    // instead of stepping into a different thread

    let load_task = move || {
        trace!(
            "module_dynamic_import: {}, load_task running",
            file_name.as_str()
        );
        // load mod code here (in helper thread)
        let script: Option<EsScriptCode> = if let Some(loader) = &rt_arc.module_source_loader {
            loader(file_name.as_str(), ref_path.as_str())
        } else {
            None
        };

        trace!(
            "module_dynamic_import: {}, load_task: loaded",
            file_name.as_str()
        );

        rt_arc.do_in_es_event_queue(move |sm_rt| {
            // compile module / get from cache here
            // resolve or reject promise here (in event queue)
            trace!(
                "module_dynamic_import: {}, load_task: back in do_in_es_runtime_thread",
                file_name.as_str()
            );
            sm_rt.do_with_jsapi(|_rt, cx, _global| {
                // check if was cached async
                // todo replace with a bool

                trace!(
                    "module_dynamic_import: {}, load_task: back in do_in_es_runtime_thread, check cache",
                    file_name.as_str()
                );

                let is_cached = MODULE_CACHE.with(|cache_rc| {
                    let cache = &*cache_rc.borrow();
                    cache.contains(&file_name)
                });

                let closure_epr = crate::spidermonkeyruntimewrapper::consume_cached_object(closure_id);
                rooted!(in (cx) let closure_root = closure_epr.get());
                rooted!(in (cx) let mut promise_val_root = NullValue());
                rooted!(in (cx) let mut specifier_val_root = NullValue());
                rooted!(in (cx) let mut reference_private_val_root = NullValue());
                jsapi_utils::objects::get_es_obj_prop_val(cx, closure_root.handle(), "promise", promise_val_root.handle_mut()).ok().expect("could not get promise prop from closure");
                jsapi_utils::objects::get_es_obj_prop_val(cx, closure_root.handle(), "specifier", specifier_val_root.handle_mut()).ok().expect("could not get specifier prop from closure");
                jsapi_utils::objects::get_es_obj_prop_val(cx, closure_root.handle(), "reference_private", reference_private_val_root.handle_mut()).ok().expect("could not get reference_private prop from closure");
                rooted!(in (cx) let mut promise_root = promise_val_root.to_object());
                rooted!(in (cx) let mut specifier_root = specifier_val_root.to_string());


                if is_cached {
                    // resolve promise
                    trace!("dyn module {} was cached, finish import", file_name.as_str());
                    FinishDynamicModuleImport(cx, reference_private_val_root.handle().into(), specifier_root.handle().into(), promise_root.handle().into());

                } else if let Some(script_code) = script {

                    trace!("dyn module {} was loaded, compile", file_name.as_str());

                    let compiled_mod_obj_res = compile_module(
                        cx,
                        script_code.get_code(),
                        script_code.get_path(),
                    );

                    if let Ok(compiled_mod_obj) = compiled_mod_obj_res {
                        MODULE_CACHE.with(|cache_rc| {
                            let cache = &mut *cache_rc.borrow_mut();
                            let mod_epr = EsPersistentRooted::new_from_obj(cx, compiled_mod_obj);
                            cache.put(file_name.clone(), mod_epr);
                        });

                        trace!("dyn module {} was loaded, compiled and cached, finish", file_name.as_str());

                        FinishDynamicModuleImport(cx, reference_private_val_root.handle().into(), specifier_root.handle().into(), promise_root.handle().into());

                    } else {
                        // reject promise

                        trace!("dyn module {} was not compiled ok, rejecting promise", file_name.as_str());

                        let err_str= format!("module failed to compile: {}", compiled_mod_obj_res.err().unwrap().err_msg());
                        rooted!(in (cx) let mut prom_reject_val = UndefinedValue());
                        jsapi_utils::new_es_value_from_str(cx, err_str.as_str(), prom_reject_val.handle_mut());

                        trace!("rejecting dynamic module promise: failed {}", err_str);
                        jsapi_utils::promises::reject_promise(
                            cx,
                            promise_root.handle(),
                            prom_reject_val.handle(),
                        )
                            .ok()
                            .expect("promise rejection failed / 1");
                    }
                } else {
                    // reject promise
                    let err_str= format!("module not found: {}", file_name);
                    trace!("rejecting dynamic module promise: failed {}", err_str);
                    rooted!(in (cx) let mut prom_reject_val = UndefinedValue());
                    jsapi_utils::new_es_value_from_str(cx, err_str.as_str(), prom_reject_val.handle_mut());

                    jsapi_utils::promises::reject_promise(
                        cx,
                        promise_root.handle(),
                        prom_reject_val.handle(),
                    )
                        .ok()
                        .expect("promise rejection failed / 2");
                }
            });
        });
    };
    EsRuntime::add_helper_task(load_task);

    true
}

unsafe extern "C" fn set_module_metadata(
    cx: *mut JSContext,
    private_value: RawHandleValue,
    meta_object: RawHandleObject,
) -> bool {
    // the goal here is to set the "url" prop on meta_object which is the full_path prop of private_value
    // i think :)

    // lets just see what we get here first
    let path = get_path_from_module_private(cx, private_value);

    rooted!(in (cx) let mut path_root = UndefinedValue());
    jsapi_utils::new_es_value_from_str(cx, path.as_str(), path_root.handle_mut());

    jsapi_utils::objects::set_es_obj_prop_value_raw(
        cx,
        meta_object,
        "url",
        path_root.handle().into(),
    );

    true
}

/// native function used a import function for module loading
unsafe extern "C" fn import_module(
    cx: *mut JSContext,
    reference_private: RawHandleValue,
    specifier: RawHandle<*mut JSString>,
) -> *mut JSObject {
    let file_name = jsapi_utils::es_jsstring_to_string(cx, *specifier);
    let ref_path = get_path_from_module_private(cx, reference_private);

    trace!("import_module {} from ref {}", file_name, ref_path);

    // see if we have that module
    let cached: Option<*mut JSObject> = MODULE_CACHE.with(|cache_rc| {
        let cache = &mut *cache_rc.borrow_mut();
        if let Some(mpr) = cache.get(&file_name) {
            trace!("found a cached module for {}", &file_name);
            // set rval here
            return Some(mpr.get());
        }
        None
    });
    if let Some(c) = cached {
        return c;
    };

    // see if we got a module code loader
    let module_code_opt: Option<EsScriptCode> = SM_RT.with(|sm_rt_rc| {
        let sm_rt = sm_rt_rc.borrow();
        let es_rt_inner = sm_rt.clone_esrt_inner();
        if let Some(module_source_loader) = &es_rt_inner.module_source_loader {
            module_source_loader(file_name.as_str(), ref_path.as_str())
        } else {
            None
        }
    });

    if let Some(module_code) = module_code_opt {
        let compiled_mod_obj_res = jsapi_utils::modules::compile_module(
            cx,
            module_code.get_code(),
            module_code.get_path(),
        );

        if compiled_mod_obj_res.is_err() {
            let err = compiled_mod_obj_res.err().unwrap();
            let err_str = format!("error loading module: {}", err.err_msg());
            log::debug!("error loading module, returning null: {}", err_str);
            report_exception2(cx, err_str);
            return *ptr::null_mut::<*mut JSObject>();
        }

        let compiled_module: *mut JSObject = compiled_mod_obj_res.ok().unwrap();

        MODULE_CACHE.with(|cache_rc| {
            trace!("caching module for {}", &file_name);
            let cache = &mut *cache_rc.borrow_mut();
            let mut mpr = EsPersistentRooted::default();
            mpr.init(cx, compiled_module);
            cache.put(file_name, mpr);
        });

        compiled_module
    } else {
        NullValue().to_object()
    }
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

    #[test]
    // run again, see if cached works
    fn test_dynamic_import5() {
        log::info!("test: test_dynamic_import5");
        let _ = test_with_sm_rt(|sm_rt| {
            let eval_res = sm_rt.load_module(
                "let test_dynamic_import_mod5_prom = import('foo_test_mod_dyn.mes').then((res) => {return ('ok' + res.other);})\
                              .catch((pex) => {return ('err' + pex);});\
                              test_dynamic_import_mod5_prom;",
                "test_dynamic_import5.mes",
            );

            match eval_res {
                Ok(ok_esvf) => ok_esvf,
                Err(err) => panic!("script failed 2: {}", err.err_msg()),
            }
        });
        std::thread::sleep(Duration::from_secs(5));
        log::debug!("test_dynamic_import5: import should be done now");
        // 'foo_test_mod.mes'
    }
}
