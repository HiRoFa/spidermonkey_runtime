use crate::jsapi_utils;
use crate::jsapi_utils::objects::get_es_obj_prop_val;
use crate::jsapi_utils::{get_pending_exception, get_type_of, EsErrorInfo};
use log::trace;
use mozjs::jsapi::CallArgs;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFreeOp;
use mozjs::jsapi::JSFunction;
use mozjs::jsapi::JSNative;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JSType;
use mozjs::jsapi::JS_CallFunction;
use mozjs::jsapi::JS_CallFunctionName;
use mozjs::jsapi::JS_CallFunctionValue;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_NewFunction;
use mozjs::jsapi::JS_NewObject;
use mozjs::jsapi::JS_ObjectIsFunction;
use mozjs::jsapi::MutableHandleObject as RawMutableHandleObject;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsval::JSVal;
use mozjs::jsval::UndefinedValue;
use mozjs::rust::{
    HandleFunction, HandleObject, HandleValue, MutableHandleFunction, MutableHandleObject,
    MutableHandleValue,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;

pub fn compile_function(
    cx: *mut JSContext,
    async_fn: bool,
    name: &str,
    body: &str,
    arg_names: Vec<&str>,
    rval: MutableHandleFunction,
) -> Result<(), EsErrorInfo> {
    trace!("compile_function / 1");

    // todo validate argNames
    // todo validate body

    let as_pfx = if async_fn { "async " } else { "" };
    let args_str = arg_names.join(", ");
    let src = format!(
        "({}function {}({}) {{\n{}\n}});",
        as_pfx, name, args_str, body
    );

    let file_name = format!("compile_func_{}.es", name);
    rooted!(in (cx) let mut script_val = ptr::null_mut::<mozjs::jsapi::JSScript>());
    let compile_res = jsapi_utils::scripts::compile_script(
        cx,
        src.as_str(),
        file_name.as_str(),
        script_val.handle_mut(),
    );
    if let Some(err) = compile_res.err() {
        return Err(err);
    }

    rooted!(in (cx) let mut func_val = UndefinedValue());
    let eval_res =
        jsapi_utils::scripts::execute_script(cx, script_val.handle(), func_val.handle_mut());
    if let Some(err) = eval_res.err() {
        return Err(err);
    }

    assert!(value_is_function(cx, *func_val));

    let mut rval = rval;
    let func_obj: *mut JSObject = func_val.to_object();
    rval.set(func_obj as *mut JSFunction);

    Ok(())
}

/// call a function by namespace and name
pub fn call_function_name(
    context: *mut JSContext,
    scope: HandleObject,
    function_name: &str,
    args: Vec<JSVal>,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    call_function_name2(
        context,
        scope,
        function_name,
        arguments_value_array,
        ret_val,
    )
}

/// call a function by name with a rooted arguments array
pub fn call_function_name2(
    context: *mut JSContext,
    scope: HandleObject,
    function_name: &str,
    args: HandleValueArray,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    trace!("call_function_name2: {}", function_name);

    let n = format!("{}\0", function_name);

    if unsafe {
        JS_CallFunctionName(
            context,
            scope.into(),
            n.as_ptr() as *const libc::c_char,
            &args,
            ret_val.into(),
        )
    } {
        Ok(())
    } else if let Some(err) = get_pending_exception(context) {
        Err(err)
    } else {
        Err(EsErrorInfo {
            message: "unknown error".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        })
    }
}

/// call a function
pub fn call_function(
    context: *mut JSContext,
    this_obj: HandleObject,
    function: HandleFunction,
    args: Vec<JSVal>,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    call_function2(context, this_obj, function, arguments_value_array, ret_val)
}

/// call a function by value
pub fn call_function_value(
    context: *mut JSContext,
    this_obj: HandleObject,
    function_val: HandleValue,
    args: Vec<JSVal>,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    call_function_value2(
        context,
        this_obj,
        function_val,
        arguments_value_array,
        ret_val,
    )
}

/// call a function with a rooted arguments array
pub fn call_function2(
    context: *mut JSContext,
    this_obj: HandleObject,
    function: HandleFunction,
    args: HandleValueArray,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    if unsafe {
        JS_CallFunction(
            context,
            this_obj.into(),
            function.into(),
            &args,
            ret_val.into(),
        )
    } {
        Ok(())
    } else if let Some(err) = get_pending_exception(context) {
        Err(err)
    } else {
        Err(EsErrorInfo {
            message: "unknown error".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        })
    }
}

/// call a function by name with a rooted arguments array
pub fn call_function_value2(
    context: *mut JSContext,
    this_obj: HandleObject,
    function_val: HandleValue,
    args: HandleValueArray,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    if unsafe {
        JS_CallFunctionValue(
            context,
            this_obj.into(),
            function_val.into(),
            &args,
            ret_val.into(),
        )
    } {
        Ok(())
    } else if let Some(err) = get_pending_exception(context) {
        Err(err)
    } else {
        Err(EsErrorInfo {
            message: "unknown error".to_string(),
            filename: "".to_string(),
            lineno: 0,
            column: 0,
        })
    }
}

/// call a function by namespace and name
pub fn call_namespace_function_name(
    context: *mut JSContext,
    scope: HandleObject,
    namespace: Vec<&str>,
    function_name: &str,
    args: Vec<JSVal>,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    let arguments_value_array = unsafe { HandleValueArray::from_rooted_slice(&*args) };

    trace!("call_namespace_function_name: {}", function_name);

    // root the args here
    rooted!(in(context) let _argument_object = unsafe {JS_NewArrayObject(context, &arguments_value_array)});

    call_namespace_function_name2(
        context,
        scope,
        namespace,
        function_name,
        arguments_value_array,
        ret_val,
    )
}

/// call a function by name on an object by name
/// e.g. esses.cleanup() can be called by calling
/// call_namespace_function_name(cx, glob, vec!["esses"], "cleanup", vec![]);
#[allow(dead_code)]
pub fn call_namespace_function_name2(
    context: *mut JSContext,
    scope: HandleObject,
    obj_names: Vec<&str>,
    function_name: &str,
    arguments_value_array: HandleValueArray,
    ret_val: MutableHandleValue,
) -> Result<(), EsErrorInfo> {
    trace!("call_namespace_function_name2: {}", function_name);

    let mut sub_scope: *mut JSObject = *scope;
    for obj_name in obj_names {
        rooted!(in(context) let sub_scope_root = sub_scope);
        rooted!(in(context) let mut new_subscope_root = UndefinedValue());
        let res = get_es_obj_prop_val(
            context,
            sub_scope_root.handle(),
            obj_name,
            new_subscope_root.handle_mut(),
        );

        if res.is_err() {
            panic!(
                "could not get prop {}: {}",
                obj_name,
                res.err().unwrap().message
            );
        }

        let val: JSVal = *new_subscope_root.handle();

        if !val.is_object() {
            return Err(EsErrorInfo {
                message: format!("{} was not an object.", obj_name),
                column: 0,
                lineno: 0,
                filename: "".to_string(),
            });
        }

        sub_scope = val.to_object();
    }

    rooted!(in(context) let sub_scope_root = sub_scope);

    call_function_name2(
        context,
        sub_scope_root.handle(),
        function_name,
        arguments_value_array,
        ret_val,
    )
}

/// check whether an Value is a function
pub fn value_is_function(context: *mut JSContext, val: JSVal) -> bool {
    let js_type = get_type_of(context, val);
    js_type == JSType::JSTYPE_FUNCTION
}

/// check whether an Object is a function
pub fn object_is_function(obj: *mut JSObject) -> bool {
    unsafe { JS_ObjectIsFunction(obj) }
}

/// define a new native function on an object
/// JSNative = Option<unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool>
// todo refactor to accept MutableHandleValue #26
pub fn define_native_function(
    cx: *mut JSContext,
    obj: HandleObject,
    function_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", function_name);

    let ret: *mut JSFunction = unsafe {
        JS_DefineFunction(
            cx,
            obj.into(),
            n.as_ptr() as *const libc::c_char,
            native_function,
            1,
            0,
        )
    };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

/// define a new native function on an object
/// JSNative = Option<unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool>
// todo refactor #27
pub fn define_native_constructor(
    cx: *mut JSContext,
    obj: HandleObject,
    constructor_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", constructor_name);

    let ret: *mut JSFunction = unsafe {
        JS_DefineFunction(
            cx,
            obj.into(),
            n.as_ptr() as *const libc::c_char,
            native_function,
            1,
            mozjs::jsapi::JSFUN_CONSTRUCTOR,
        )
    };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

/// define a new native function
/// JSNative = Option<unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool>
// todo refactor #28
pub fn new_native_function(
    cx: *mut JSContext,
    function_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", function_name);

    let ret: *mut JSFunction =
        unsafe { JS_NewFunction(cx, native_function, 1, 0, n.as_ptr() as *const libc::c_char) };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

pub fn new_native_constructor(
    cx: *mut JSContext,
    constructor_name: &str,
    native_function: JSNative,
) -> *mut JSFunction {
    let n = format!("{}\0", constructor_name);

    let ret: *mut JSFunction = unsafe {
        JS_NewFunction(
            cx,
            native_function,
            1,
            mozjs::jsapi::JSFUN_CONSTRUCTOR,
            n.as_ptr() as *const libc::c_char,
        )
    };

    ret

    //https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey/JSAPI_reference/JS_DefineFunction
}

static CALLBACK_CLASS_OPS: JSClassOps = JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: None,
    mayResolve: None,
    finalize: Some(finalize_callback),
    call: Some(call_callback),
    hasInstance: None,
    construct: None,
    trace: None,
};

unsafe extern "C" fn call_callback(cx: *mut JSContext, argc: u32, vp: *mut JSVal) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    let callback_obj: *mut JSObject = args.callee();
    let callback_id = callback_obj as usize;
    trace!("call callback id: {}", callback_id);

    CALLBACKS.with(|callbacks_rc| {
        let callbacks = &mut *callbacks_rc.borrow_mut();
        if callbacks.contains_key(&callback_id) {
            trace!("found callback");
            let callback = callbacks.get(&callback_id).unwrap();
            let mut args_vec = vec![];
            for x in 0..args.argc_ {
                args_vec.push(HandleValue::from_marked_location(&*args.get(x)));
            }

            let res = callback(
                cx,
                args_vec,
                crate::jsapi_utils::handles::from_raw_handle_mut(args.rval()),
            );
            match res {
                Ok(_) => {
                    trace!("callback succeeded");
                    true
                }
                Err(e) => {
                    let s = format!("error while invoking callback: {}", e);
                    trace!("{}", s);
                    crate::jsapi_utils::report_exception2(cx, s);

                    false
                }
            }
        } else {
            trace!("callback not found for id {}", callback_id);
            let s = format!("callback not found for id {}", callback_id);
            crate::jsapi_utils::report_exception2(cx, s);
            false
        }
    })
}
unsafe extern "C" fn finalize_callback(_op: *mut JSFreeOp, callback_obj: *mut JSObject) {
    let callback_id = callback_obj as usize;
    trace!("finalize callback id: {}", callback_id);
    CALLBACKS.with(|callbacks_rc| {
        let callbacks = &mut *callbacks_rc.borrow_mut();
        trace!("callback exists = {}", callbacks.contains_key(&callback_id));
        callbacks.remove(&callback_id);
    });
}

static CALLBACK_CLASS: JSClass = JSClass {
    name: b"RustCallback\0" as *const u8 as *const libc::c_char,
    flags: mozjs::jsapi::JSCLASS_FOREGROUND_FINALIZE,
    cOps: &CALLBACK_CLASS_OPS as *const JSClassOps,
    spec: ptr::null(),
    ext: ptr::null(),
    oOps: ptr::null(),
};

pub type Callback =
    dyn Fn(*mut JSContext, Vec<HandleValue>, MutableHandleValue) -> Result<(), String> + 'static;

thread_local! {
    static CALLBACKS: RefCell<HashMap<usize, Box<Callback>>> = RefCell::new(HashMap::new());
}

pub fn new_callback<C>(cx: *mut JSContext, rval: MutableHandleObject, callback: C) -> bool
where
    C: Fn(*mut JSContext, Vec<HandleValue>, MutableHandleValue) -> Result<(), String> + 'static,
{
    new_callback_raw(cx, rval.into(), callback)
}

pub fn new_callback_raw<C>(cx: *mut JSContext, rval: RawMutableHandleObject, callback: C) -> bool
where
    C: Fn(*mut JSContext, Vec<HandleValue>, MutableHandleValue) -> Result<(), String> + 'static,
{
    // create callback obj

    let callback_obj: *mut JSObject = unsafe { JS_NewObject(cx, &CALLBACK_CLASS) };

    rval.set(callback_obj);
    let callback_id = callback_obj as usize;

    CALLBACKS.with(move |callbacks_rc| {
        let callbacks = &mut *callbacks_rc.borrow_mut();
        trace!("inserting callback with id {}", callback_id);
        callbacks.insert(callback_id, Box::new(callback));
    });

    true
}

#[cfg(test)]
mod tests {
    use crate::jsapi_utils;
    use crate::jsapi_utils::functions::{
        call_function, call_function_name, call_namespace_function_name, compile_function,
        new_callback, value_is_function,
    };
    use crate::jsapi_utils::get_pending_exception;
    use crate::jsapi_utils::tests::test_with_sm_rt;
    use log::trace;
    use mozjs::jsapi::JSFunction;
    use mozjs::jsval::{Int32Value, JSVal, ObjectValue, UndefinedValue};
    use std::ptr;
    use std::time::Duration;

    #[test]
    fn test_instance_of_function() {
        log::info!("test: test_instance_of_function");
        let res = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());
                println!("evalling new func");
                let res = jsapi_utils::eval(
                    rt,
                    global,
                    "(function test_func(){});",
                    "test_instance_of_function.es",
                    rval.handle_mut(),
                );
                if res.is_err() {
                    if let Some(err) = get_pending_exception(cx) {
                        println!("err: {}", err.message);
                    }
                } else {
                    println!("getting value");
                    let p_value: mozjs::jsapi::Value = *rval;
                    println!("getting obj {}", p_value.is_object());
                    return value_is_function(cx, p_value);
                }
                false
            })
        });
        assert_eq!(res, true);
    }

    #[test]
    fn test_function_by_name() {
        log::info!("test: test_function_by_name");
        let ret = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {
                rooted!(in(cx) let mut rval = UndefinedValue());

                let _res = jsapi_utils::eval(
                    rt,
                    global,
                    "this.test_function_by_name_func = function(a, b){return a * b;};",
                    "test_function_by_name.es",
                    rval.handle_mut(),
                );

                let a: JSVal = Int32Value(7);
                let b: JSVal = Int32Value(5);
                let fres = call_function_name(
                    cx,
                    global,
                    "test_function_by_name_func",
                    vec![a, b],
                    rval.handle_mut(),
                );
                if fres.is_err() {
                    panic!(fres.err().unwrap().message);
                }
                let ret_val: JSVal = *rval;
                let ret: i32 = ret_val.to_int32();
                ret
            })
        });

        assert_eq!(ret, 35);
    }

    #[test]
    fn test_namespace_function_by_name() {
        log::info!("test: test_namespace_function_by_name");
        for _x in 0..100 {
            test_namespace_function_by_name2();
        }
    }

    fn test_namespace_function_by_name2() {
        log::info!("test: test_namespace_function_by_name2");
        let ret = test_with_sm_rt(|sm_rt| {
            sm_rt.do_with_jsapi(|rt, cx, global| {

                rooted!(in(cx) let mut rval = UndefinedValue());

                let res = jsapi_utils::eval(
                    rt,
                    global,
                    "this.test_namespace_function_by_name2 = {test_namespace_function_by_name2_func :function(a, b){return a * b;}};", "test_namespace_function_by_name2.es",
                    rval.handle_mut()
                );

                if res.is_err() {
                    let err_res = get_pending_exception(cx);
                    if let Some(err) = err_res {
                        println!("err {}", err.message);
                    }
                }

                let a: JSVal = Int32Value(7);
                let b: JSVal = Int32Value(5);
                let fres = call_namespace_function_name(
                    cx,
                    global,
                    vec!["test_namespace_function_by_name2"],
                    "test_namespace_function_by_name2_func",
                    vec![a, b],
                    rval.handle_mut(),
                );
                if fres.is_err() {
                    panic!(fres.err().unwrap().message);
                }
                let ret_val: JSVal = *rval;
                let ret: i32 = ret_val.to_int32();
                ret
            })
        });

        assert_eq!(ret, 35);
    }

    #[test]
    fn test_callback() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.eval_sync(
            "this.test_callback_func = function(cb){cb();};",
            "test_callback.es",
        )
        .ok()
        .expect("eval failed");

        rt.do_in_es_event_queue_sync(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, cx, global| {
                rooted!(in (cx) let mut cb_root = mozjs::jsval::NullValue().to_object_or_null());
                new_callback(cx, cb_root.handle_mut(), |_cx, _args, _rval| {
                    trace!("callback closure was called");
                    Ok(())
                });
                rooted!(in (cx) let func_val = ObjectValue(*cb_root));
                rooted!(in (cx) let mut frval = UndefinedValue());
                call_function_name(
                    cx,
                    global,
                    "test_callback_func",
                    vec![*func_val],
                    frval.handle_mut(),
                )
                .ok()
                .expect("call method failed");
            });
        });

        rt.cleanup_sync();

        std::thread::sleep(Duration::from_secs(5));
        trace!("end of test_callback");
    }

    #[test]
    fn test_compile_func() {
        let rt = crate::esruntime::tests::TEST_RT.clone();
        rt.do_in_es_event_queue_sync(|sm_rt| {
            sm_rt.do_with_jsapi(|_rt, cx, global| {
                rooted!(in (cx) let mut function_root = ptr::null_mut::<JSFunction>());
                trace!("compiling function");
                let compile_res = compile_function(
                    cx,
                    false,
                    "my_func",
                    "return a * b;",
                    vec!["a", "b"],
                    function_root.handle_mut(),
                );
                if let Some(err) = compile_res.err() {
                    panic!("could not compile function: {}", err.err_msg());
                }

                trace!("executing function");
                rooted!(in (cx) let mut frval = UndefinedValue());
                rooted!(in (cx) let a = Int32Value(13));
                rooted!(in (cx) let b = Int32Value(3));

                call_function(
                    cx,
                    global,
                    function_root.handle(),
                    vec![*a, *b],
                    frval.handle_mut(),
                )
                .ok()
                .expect("func failed");

                assert!(frval.is_int32());
                assert_eq!(frval.to_int32(), 39);
                trace!("executed function");
            });
        });
    }
}
