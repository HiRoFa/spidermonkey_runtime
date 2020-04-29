use crate::es_utils::es_jsid_to_string;
use crate::es_utils::rooting::EsPersistentRooted;
use core::ptr;
use log::trace;

use mozjs::jsapi::CallArgs;
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFreeOp;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_NewArrayObject;
use mozjs::jsapi::JS_ReportErrorASCII;
use mozjs::jsapi::JS::HandleValueArray;
use mozjs::jsapi::JSCLASS_FOREGROUND_FINALIZE;
use mozjs::jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::{HandleObject, HandleValue};

use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ptr::replace;
use std::sync::Arc;

/// create a class def in the runtime which constructs and calls methods in a rust proxy
///

pub struct Proxy {
    class_name: String,
    constructor: Option<Box<dyn Fn(*mut JSContext, &CallArgs) -> Result<i32, String>>>,
    finalizer: Option<Box<dyn Fn(&i32) -> ()>>,
    properties: HashMap<
        &'static str,
        (
            Box<dyn Fn(i32) -> JSVal>,
            Box<dyn Fn(i32, HandleValue) -> ()>,
        ),
    >,
    methods: HashMap<&'static str, Box<dyn Fn(i32, &CallArgs) -> JSVal>>,
    events: HashSet<&'static str>,
}

pub struct ProxyBuilder {
    class_name: String,
    constructor: Option<Box<dyn Fn(*mut JSContext, &CallArgs) -> Result<i32, String>>>,
    finalizer: Option<Box<dyn Fn(&i32) -> ()>>,
    properties: HashMap<
        &'static str,
        (
            Box<dyn Fn(i32) -> JSVal>,
            Box<dyn Fn(i32, HandleValue) -> ()>,
        ),
    >,
    methods: HashMap<&'static str, Box<dyn Fn(i32, &CallArgs) -> JSVal>>,
    events: HashSet<&'static str>,
}

thread_local! {
    static PROXY_INSTANCE_IDS: RefCell<HashMap<usize, i32>> = RefCell::new(HashMap::new());
    static PROXY_INSTANCE_CLASSNAMES: RefCell<HashMap<i32, String>> = RefCell::new(HashMap::new());
    static PROXIES: RefCell<HashMap<String, Arc<Proxy>>> = RefCell::new(HashMap::new());
}

pub fn get_proxy(name: &str) -> Option<Arc<Proxy>> {
    // get proxy from PROXIES
    PROXIES.with(|rc: &RefCell<HashMap<String, Arc<Proxy>>>| {
        let map: &HashMap<String, Arc<Proxy>> = &*rc.borrow();
        map.get(name).cloned()
    })
}

impl Proxy {
    fn new(cx: *mut JSContext, scope: HandleObject, builder: &mut ProxyBuilder) -> Arc<Self> {
        let mut ret = Proxy {
            class_name: builder.class_name.to_string(),
            constructor: unsafe { replace(&mut builder.constructor, None) },
            finalizer: unsafe { replace(&mut builder.finalizer, None) },
            properties: HashMap::new(),
            methods: HashMap::new(),
            events: HashSet::new(),
        };

        builder.properties.drain().all(|e| {
            ret.properties.insert(e.0, e.1);
            true
        });

        builder.methods.drain().all(|e| {
            ret.methods.insert(e.0, e.1);
            true
        });

        builder.events.drain().all(|evt_type| {
            ret.events.insert(evt_type);
            true
        });

        // todo, do we create an instance of proxy and set its constructor or do we define constructor and then add all static methods to that constructor obj? plan b is simpler but does that work on a native function?

        crate::es_utils::functions::define_native_constructor(
            cx,
            scope,
            ret.class_name.as_str(),
            Some(construct),
        );

        let ret_arc = Arc::new(ret);

        PROXIES.with(|map_rc: &RefCell<HashMap<String, Arc<Proxy>>>| {
            let map = &mut *map_rc.borrow_mut();
            map.insert(ret_arc.class_name.clone(), ret_arc.clone());
        });

        ret_arc
    }

    pub fn new_instance(args: Vec<HandleValue>) -> *mut JSObject {
        panic!("NYI");
    }

    pub fn dispatch_event(obj_id: i32, event_name: &str, event_obj: HandleObject) {
        panic!("NYI");
    }

    pub fn dispatch_static_event(event_name: &str, event_obj: HandleObject) {
        panic!("NYI");
    }

    pub fn invoke_method(obj_id: i32, method_name: &str, args: Vec<HandleValue>) {
        panic!("NYI");
    }

    pub fn invoke_static_method(method_name: &str, args: Vec<HandleValue>) {
        panic!("NYI");
    }

    fn static_get(prop_name: &str) -> JSVal {
        // invoke this from C fn getter
        panic!("NYI");
    }

    fn static_set(prop_name: &str, val: JSVal) -> () {
        // invoke this from C fn setter
        panic!("NYI");
    }

    fn add_event_listener(obj_id: i32, event_type: &str, listener: HandleObject) {
        // assert that listener is a function?
        panic!("NYI");
    }

    fn remove_event_listener(obj_id: i32, event_type: &str, listener: HandleObject) {
        // assert that listener is a function?
        panic!("NYI");
    }

    fn add_static_event_listener(event_type: &str, listener: HandleObject) {
        // assert that listener is a function?
        panic!("NYI");
    }

    fn remove_static_event_listener(event_type: &str, listener: HandleObject) {
        // assert that listener is a function?
        panic!("NYI");
    }

    /*fn init_properties(&self, cx: *mut JSContext, func: HandleObject) {
        // this is actually how static_props should work, not instance props.. they should be resolved from the proxy_op
        for prop_name in self.properties.keys() {
            // https://doc.servo.org/mozjs/jsapi/fn.JS_DefineProperty1.html
            // mozjs::jsapi::JS_DefineProperty1
            // todo move this to es_utils::object
            let n = format!("{}\0", prop_name);

            let ok = unsafe {
                mozjs::jsapi::JS_DefineProperty1(
                    cx,
                    func,
                    n.as_ptr() as *const libc::c_char,
                    JSPROP_PERMANENT + JSPROP_GETTER + JSPROP_SETTER + JSPROP_SHARED,
                    Some(getter),
                    Some(setter),
                )
            };
        }
    }*/
}

impl ProxyBuilder {
    pub fn new(class_name: &str) -> Self {
        ProxyBuilder {
            class_name: class_name.to_string(),
            constructor: None,
            finalizer: None,
            properties: HashMap::new(),
            methods: HashMap::new(),
            events: HashSet::new(),
        }
    }

    pub fn constructor<C>(&mut self, constructor: C) -> &mut Self
    where
        C: Fn(*mut JSContext, &CallArgs) -> Result<i32, String> + 'static,
    {
        self.constructor = Some(Box::new(constructor));
        self
    }

    pub fn finalizer<F>(&mut self, finalizer: F) -> &mut Self
    where
        F: Fn(&i32) -> () + 'static,
    {
        self.finalizer = Some(Box::new(finalizer));
        self
    }

    pub fn property<G, S>(&mut self, name: &'static str, getter: G, setter: S) -> &mut Self
    where
        G: Fn(i32) -> JSVal + 'static,
        S: Fn(i32, HandleValue) -> () + 'static,
    {
        self.properties
            .insert(name, (Box::new(getter), Box::new(setter)));
        self
    }

    pub fn method<M>(&mut self, name: &'static str, method: M) -> &mut Self
    where
        M: Fn(i32, &CallArgs) -> JSVal + 'static,
    {
        self.methods.insert(name, Box::new(method));
        self
    }

    pub fn build(&mut self, cx: *mut JSContext, scope: HandleObject) -> Arc<Proxy> {
        Proxy::new(cx, scope, self)
    }

    pub fn event(&mut self, evt_type: &'static str) -> &mut Self {
        self.events.insert(evt_type);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::es_utils::es_value_to_str;
    use crate::es_utils::reflection::*;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use log::debug;
    use mozjs::jsapi::CallArgs;
    use mozjs::jsval::{Int32Value, UndefinedValue};

    #[test]
    fn test_proxy() {
        log::info!("test_proxy");
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntime| {
                sm_rt.do_with_jsapi(|_rt, cx, global| {
                    let _proxy_arc = ProxyBuilder::new("TestClass1")
                        .constructor(|cx: *mut JSContext, args: &CallArgs| {
                            // this will run in the sm_rt workerthread so global is rooted here
                            debug!("proxytest: construct");
                            let foo;
                            if args.argc_ > 0 {
                                let hv = args.index(0);
                                foo = es_value_to_str(cx, &*hv).ok().unwrap();
                            } else {
                                foo = "NoName".to_string();
                            }
                            debug!("proxytest: construct with name {}", foo);
                            Ok(1)
                        })
                        .property("foo", |obj_id| {
                            debug!("proxy.get foo {}", obj_id);
                            Int32Value(123)
                        }, |obj_id, _val| {
                            debug!("proxy.set foo {}", obj_id);
                        })
                        .property("bar", |_obj_id| {
                            Int32Value(456)
                        }, |_obj_id, _val| {})
                        .finalizer(|id: &i32| {
                            debug!("proxytest: finalize id {}", id);
                        })
                        .method("methodA", |obj_id, args| {
                            trace!("proxy.methodA called for obj {} with {} args", obj_id, args.argc_);
                            UndefinedValue()
                        })
                        .method("methodB", |obj_id, args| {
                            trace!("proxy.methodB called for obj {} with {} args", obj_id, args.argc_);
                            UndefinedValue()
                        })
                        .build(cx, global);
                    let esvf = sm_rt
                        .eval(
                            "let tp_obj = new TestClass1('bar'); tp_obj.abc = 1; console.log('tp_obj.abc = %s', tp_obj.abc); let i = tp_obj.foo; tp_obj.foo = 987; tp_obj.methodA(1, 2, 3); tp_obj.methodB(true); tp_obj.addEventListener('saved', (evt) => {console.log('tp_obj was saved');}); tp_obj.dispatchEvent('saved', {}); tp_obj = null; i;",
                            "test_proxy.es",
                        )
                        .ok()
                        .unwrap();
                    assert_eq!(&123, esvf.get_i32());
                });
            });
            inner.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntime| {
                sm_rt.cleanup();
            });
        });
    }
}

// todo, test resolve methods and such
// todo should this wrap a native variant? with callargs and such
// and thus should i impl a native variant first in es_utils?
// yes.. yes i should...
// should allways impl https://developer.mozilla.org/en-US/docs/Web/API/EventTarget

static ES_PROXY_CLASS_CLASS_OPS: JSClassOps = JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: Some(resolve),
    mayResolve: None,
    finalize: Some(finalize),
    call: None,
    hasInstance: None,
    construct: None,
    trace: None,
};

static ES_PROXY_CLASS: JSClass = JSClass {
    name: b"EsProxy\0" as *const u8 as *const libc::c_char,
    flags: JSCLASS_FOREGROUND_FINALIZE,
    cOps: &ES_PROXY_CLASS_CLASS_OPS as *const JSClassOps,
    spec: ptr::null(),
    ext: ptr::null(),
    oOps: ptr::null(),
};

/// resolvea property, this means if we know how to handle a prop we define that prop ob the instance obj
unsafe extern "C" fn resolve(
    cx: *mut JSContext,
    obj: mozjs::jsapi::HandleObject,
    key: mozjs::jsapi::HandleId,
    resolved: *mut bool,
) -> bool {
    trace!("reflection::resolve");

    let prop_name = es_jsid_to_string(cx, key);

    trace!("reflection::resolve {}", prop_name);

    let rhandle = mozjs::rust::HandleObject::from_marked_location(&obj.get());
    let class_name_res =
        crate::es_utils::objects::get_es_obj_prop_val_as_string(cx, rhandle, PROXY_PROP_CLASS_NAME);
    if let Some(class_name) = class_name_res.ok() {
        PROXIES.with(|proxies_rc| {
            let proxies = &*proxies_rc.borrow();
            if let Some(proxy) = proxies.get(&class_name) {
                trace!("check proxy {} for {}", class_name, prop_name);

                if prop_name.as_str().eq("addEventListener") {
                    trace!("define addEventListener");

                    let robj = mozjs::rust::HandleObject::from_marked_location(&obj.get());
                    crate::es_utils::functions::define_native_function(
                        cx,
                        robj,
                        "addEventListener",
                        Some(add_event_listener),
                    );

                    *resolved = true;
                    trace!("resolved addEventListener {}", prop_name);
                } else if prop_name.as_str().eq("removeEventListener") {
                    trace!("define removeEventListener");

                    let robj = mozjs::rust::HandleObject::from_marked_location(&obj.get());
                    crate::es_utils::functions::define_native_function(
                        cx,
                        robj,
                        "removeEventListener",
                        Some(remove_event_listener),
                    );

                    *resolved = true;
                    trace!("resolved removeEventListener {}", prop_name);
                } else if prop_name.as_str().eq("dispatchEvent") {
                    trace!("define dispatchEvent");

                    let robj = mozjs::rust::HandleObject::from_marked_location(&obj.get());
                    crate::es_utils::functions::define_native_function(
                        cx,
                        robj,
                        "dispatchEvent",
                        Some(dispatch_event),
                    );

                    *resolved = true;
                    trace!("resolved dispatchEvent {}", prop_name);
                } else if proxy.properties.contains_key(prop_name.as_str()) {
                    trace!(
                        "define prop for proxy {} for name {}",
                        class_name,
                        prop_name
                    );

                    let n = format!("{}\0", prop_name);

                    // todo move this to es_utils (objects::define_native_getter_setter)
                    let ok = mozjs::jsapi::JS_DefineProperty1(
                        cx,
                        obj,
                        n.as_ptr() as *const libc::c_char,
                        Some(getter),
                        Some(setter),
                        (mozjs::jsapi::JSPROP_PERMANENT
                            & mozjs::jsapi::JSPROP_GETTER
                            & mozjs::jsapi::JSPROP_SETTER) as u32,
                    );
                    if !ok {
                        panic!("could not define prop");
                    }

                    *resolved = true;

                    trace!("resolved prop {}", prop_name);
                } else if proxy.methods.contains_key(prop_name.as_str()) {
                    trace!(
                        "define method for proxy {} for name {}",
                        class_name,
                        prop_name
                    );

                    let robj = mozjs::rust::HandleObject::from_marked_location(&obj.get());
                    crate::es_utils::functions::define_native_function(
                        cx,
                        robj,
                        prop_name.as_str(),
                        Some(method),
                    );

                    *resolved = true;
                    trace!("resolved method {}", prop_name);
                }
            }
        });
    }

    true
}

unsafe extern "C" fn getter(cx: *mut JSContext, argc: u32, vp: *mut mozjs::jsapi::Value) -> bool {
    trace!("reflection::getter");

    let args = CallArgs::from_vp(vp, argc);
    let thisv: mozjs::jsapi::Value = *args.thisv();

    if thisv.is_object() {
        let obj_handle = mozjs::rust::HandleObject::from_marked_location(&thisv.to_object());
        let cn_res = crate::es_utils::objects::get_es_obj_prop_val_as_string(
            cx,
            obj_handle,
            PROXY_PROP_CLASS_NAME,
        );
        if let Some(class_name) = cn_res.ok() {
            trace!("reflection::getter get for cn:{}", class_name);

            let callee: *mut JSObject = args.callee();
            let prop_name_res = crate::es_utils::objects::get_es_obj_prop_val_as_string(
                cx,
                HandleObject::from_marked_location(&callee),
                "name",
            );
            if let Some(prop_name) = prop_name_res.ok() {
                // lovely the name here is "get [propname]"
                trace!("reflection::getter get {} for cn:{}", prop_name, class_name);

                // get obj id
                let obj_id = crate::es_utils::objects::get_es_obj_prop_val_as_i32(
                    cx,
                    obj_handle,
                    PROXY_PROP_OBJ_ID,
                );

                trace!(
                    "reflection::getter get {} for cn:{} for obj_id {}",
                    prop_name,
                    class_name,
                    obj_id
                );

                let p_name = &prop_name[4..];

                PROXIES.with(|proxies_rc| {
                    let proxies = &*proxies_rc.borrow();
                    if let Some(proxy) = proxies.get(&class_name).cloned() {
                        if let Some(prop) = proxy.properties.get(p_name) {
                            let js_val = prop.0(obj_id);
                            trace!("got val for getter");
                            args.rval().set(js_val);
                        }
                    }
                });
            }
        }
    }

    true
}

fn get_obj_id_for(cx: *mut JSContext, obj: *mut JSObject) -> i32 {
    let obj_handle = unsafe { mozjs::rust::HandleObject::from_marked_location(&obj) };
    crate::es_utils::objects::get_es_obj_prop_val_as_i32(cx, obj_handle, PROXY_PROP_OBJ_ID)
}

fn get_proxy_for(cx: *mut JSContext, obj: *mut JSObject) -> Option<Arc<Proxy>> {
    let obj_handle = unsafe { mozjs::rust::HandleObject::from_marked_location(&obj) };
    let cn_res = crate::es_utils::objects::get_es_obj_prop_val_as_string(
        cx,
        obj_handle,
        PROXY_PROP_CLASS_NAME,
    );
    if let Some(class_name) = cn_res.ok() {
        return PROXIES.with(|proxies_rc| {
            let proxies = &*proxies_rc.borrow();
            proxies.get(&class_name).cloned()
        });
    }

    None
}

unsafe extern "C" fn setter(cx: *mut JSContext, argc: u32, vp: *mut mozjs::jsapi::Value) -> bool {
    trace!("reflection::setter");

    let args = CallArgs::from_vp(vp, argc);
    let thisv: mozjs::jsapi::Value = *args.thisv();

    if thisv.is_object() {
        if let Some(proxy) = get_proxy_for(cx, thisv.to_object()) {
            trace!("reflection::setter get for cn:{}", &proxy.class_name);

            let callee: *mut JSObject = args.callee();
            let prop_name_res = crate::es_utils::objects::get_es_obj_prop_val_as_string(
                cx,
                HandleObject::from_marked_location(&callee),
                "name",
            );
            if let Some(prop_name) = prop_name_res.ok() {
                // lovely the name here is "set [propname]"
                trace!("reflection::setter set {}", prop_name);

                // get obj id
                let obj_id = get_obj_id_for(cx, thisv.to_object());

                trace!(
                    "reflection::setter set {} for for obj_id {}",
                    prop_name,
                    obj_id
                );

                // strip "set " from propname
                let p_name = &prop_name[4..];

                if let Some(prop) = proxy.properties.get(p_name) {
                    let val = HandleValue::from_marked_location(&args.index(0).get());

                    trace!("reflection::setter setting val");
                    prop.1(obj_id, val);
                }
            }
        }
    }

    true
}

thread_local! {
    // todo don't forget to clear in finalize
    pub static PROXY_EVENT_LISTENERS: RefCell<HashMap<i32, HashMap<&'static str, Vec<EsPersistentRooted>>>> = RefCell::new(HashMap::new());
}

unsafe extern "C" fn add_event_listener(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    trace!("add_event_listener");
    // todo

    if argc >= 2 {
        let args = CallArgs::from_vp(vp, argc);
        let type_handle_val = args.index(0);
        let listener_handle_val = *args.index(1);

        let listener_obj: *mut JSObject = listener_handle_val.to_object();

        let listener_epr = EsPersistentRooted::new_from_obj(cx, listener_obj);
        let type_str = crate::es_utils::es_value_to_str(cx, &type_handle_val)
            .ok()
            .unwrap();

        let thisv: mozjs::jsapi::Value = *args.thisv();

        let obj_id = get_obj_id_for(cx, thisv.to_object());

        if let Some(proxy) = get_proxy_for(cx, thisv.to_object()) {
            if proxy.events.contains(&type_str.as_str()) {
                // we need this so we can get a &'static str
                let type_str = proxy.events.get(type_str.as_str()).unwrap().clone();

                PROXY_EVENT_LISTENERS.with(|pel_rc| {
                    let pel = &mut *pel_rc.borrow_mut();
                    if !pel.contains_key(&obj_id) {
                        pel.insert(obj_id, HashMap::new());
                    }
                    let obj_map = pel.get_mut(&obj_id).unwrap();

                    if !obj_map.contains_key(type_str) {
                        obj_map.insert(type_str, vec![]);
                    }

                    let listener_vec = obj_map.get_mut(type_str).unwrap();
                    listener_vec.push(listener_epr);
                });
            }
        }
    }

    true
}

unsafe extern "C" fn remove_event_listener(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    trace!("remove_event_listener");
    //todo
    true
}

unsafe extern "C" fn dispatch_event(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    trace!("dispatch_event");
    //todo should call proxy.dispatch_event()

    if argc >= 2 {
        let args = CallArgs::from_vp(vp, argc);
        let type_handle_val = args.index(0);
        let evt_obj_handle_val = args.index(1);

        let type_str = crate::es_utils::es_value_to_str(cx, &type_handle_val)
            .ok()
            .unwrap();

        let thisv: mozjs::jsapi::Value = *args.thisv();

        let obj_id = get_obj_id_for(cx, thisv.to_object());

        if let Some(proxy) = get_proxy_for(cx, thisv.to_object()) {
            if proxy.events.contains(&type_str.as_str()) {
                let type_str = proxy.events.get(type_str.as_str()).unwrap().clone();

                dispatch_event_for_proxy(cx, obj_id, type_str, evt_obj_handle_val);
            }
        }
    }
    true
}

// proxy can call this from Proxy::dispatch_event with esvf.to_es_val()
fn dispatch_event_for_proxy(
    cx: *mut JSContext,
    obj_id: i32,
    evt_type: &str,
    evt_obj: mozjs::jsapi::HandleValue,
) {
    PROXY_EVENT_LISTENERS.with(|pel_rc| {
        let pel = &*pel_rc.borrow();
        if let Some(obj_map) = pel.get(&obj_id) {
            if let Some(listener_vec) = obj_map.get(evt_type) {
                rooted!(in (cx) let mut ret_val = UndefinedValue());
                // todo this_obj should be the proxy obj..
                rooted!(in (cx) let this_obj = UndefinedValue().to_object_or_null());
                // since evt_obj is already rooted here we don;t need the auto_root macro, we can just use call_method_value()

                for listener_epr in listener_vec {
                    let mut args_vec = vec![];
                    args_vec.push(*evt_obj);
                    let func_obj = listener_epr.get();
                    // why do we only have a call_method by val and not by HandleObject?
                    // the whole rooting func here could be avoided
                    rooted!(in (cx) let function_val = ObjectValue(func_obj));
                    crate::es_utils::functions::call_method_value(
                        cx,
                        this_obj.handle(),
                        function_val.handle(),
                        args_vec,
                        ret_val.handle_mut(),
                    );
                }
            }
        }
    });
}

unsafe extern "C" fn method(cx: *mut JSContext, argc: u32, vp: *mut mozjs::jsapi::Value) -> bool {
    trace!("reflection::method");

    let args = CallArgs::from_vp(vp, argc);
    let thisv: mozjs::jsapi::Value = *args.thisv();

    if thisv.is_object() {
        if let Some(proxy) = get_proxy_for(cx, thisv.to_object()) {
            let obj_handle = mozjs::rust::HandleObject::from_marked_location(&thisv.to_object());

            trace!("reflection::method for cn:{}", &proxy.class_name);

            let callee: *mut JSObject = args.callee();
            let prop_name_res = crate::es_utils::objects::get_es_obj_prop_val_as_string(
                cx,
                HandleObject::from_marked_location(&callee),
                "name",
            );
            if let Some(prop_name) = prop_name_res.ok() {
                // lovely the name here is "get [propname]"
                trace!("reflection::method {}", prop_name);

                // get obj id
                let obj_id = get_obj_id_for(cx, thisv.to_object());

                trace!("reflection::method {} for for obj_id {}", prop_name, obj_id);

                let p_name = prop_name.as_str();

                if let Some(prop) = proxy.methods.get(p_name) {
                    trace!("got method for method");
                    let js_val = prop(obj_id, &args); // todo pass Vec of HandleValue instead of callargs
                    args.rval().set(js_val);
                }
            }
        }
    }

    true
}

unsafe extern "C" fn finalize(_fop: *mut JSFreeOp, object: *mut JSObject) {
    trace!("reflection::finalize");

    let ptr_usize = object as usize;
    let id_opt = PROXY_INSTANCE_IDS.with(|piid_rc| {
        let piid = &mut *piid_rc.borrow_mut();
        piid.remove(&ptr_usize)
    });

    if let Some(id) = id_opt {
        let cn = PROXY_INSTANCE_CLASSNAMES
            .with(|piid_rc| {
                let piid = &mut *piid_rc.borrow_mut();
                piid.remove(&id)
            })
            .unwrap();

        trace!("finalize id {} of type {}", id, cn);
        if let Some(proxy) = get_proxy(cn.as_str()) {
            if let Some(finalizer) = &proxy.finalizer {
                finalizer(&id);
            }
        }
    }
}

const PROXY_PROP_CLASS_NAME: &str = "__proxy_class_name__";
const PROXY_PROP_OBJ_ID: &str = "__proxy_obj_id__";

unsafe extern "C" fn construct(
    cx: *mut JSContext,
    argc: u32,
    vp: *mut mozjs::jsapi::Value,
) -> bool {
    trace!("reflection::construct");

    let args = CallArgs::from_vp(vp, argc);

    rooted!(in (cx) let constructor_root = args.calleev().to_object());

    let class_name = crate::es_utils::objects::get_es_obj_prop_val_as_string(
        cx,
        constructor_root.handle(),
        "name",
    )
    .ok()
    .unwrap();
    trace!("reflection::construct cn={}", class_name);

    if let Some(proxy) = get_proxy(class_name.as_str()) {
        trace!("constructing proxy {}", class_name);
        if let Some(constructor) = &proxy.constructor {
            trace!("constructing proxy constructor {}", class_name);
            let obj_id_res = constructor(cx, &args);

            if obj_id_res.is_ok() {
                let obj_id = obj_id_res.ok().unwrap();
                let ret: *mut JSObject = mozjs::jsapi::JS_NewObject(cx, &ES_PROXY_CLASS);

                rooted!(in (cx) let ret_root = ret);
                rooted!(in (cx) let pname_root = crate::es_utils::new_es_value_from_str(cx, &class_name));
                rooted!(in (cx) let obj_id_root = mozjs::jsval::Int32Value(obj_id));
                crate::es_utils::objects::set_es_obj_prop_val_permanent(
                    cx,
                    ret_root.handle(),
                    PROXY_PROP_CLASS_NAME,
                    pname_root.handle(),
                );
                crate::es_utils::objects::set_es_obj_prop_val_permanent(
                    cx,
                    ret_root.handle(),
                    PROXY_PROP_OBJ_ID,
                    obj_id_root.handle(),
                );

                PROXY_INSTANCE_IDS.with(|piid_rc| {
                    let piid = &mut *piid_rc.borrow_mut();
                    piid.insert(ret as usize, obj_id.clone());
                });

                PROXY_INSTANCE_CLASSNAMES.with(|piid_rc| {
                    let piid = &mut *piid_rc.borrow_mut();
                    piid.insert(obj_id.clone(), class_name.clone());
                });

                args.rval().set(ObjectValue(ret));

                return true;
            } else {
                JS_ReportErrorASCII(cx, b"constructor failed\0".as_ptr() as *const libc::c_char);

                return false;
            }
        }
    }

    JS_ReportErrorASCII(cx, b"no such class found\0".as_ptr() as *const libc::c_char);

    return false;
}
