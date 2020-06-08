//! # EsProxy
//!
//! the EsProxy struct provides a simple way to reflect a rust object in teh script engine
//!
//! # Example
//!
//! ```rust
//!
//! use es_runtime::esreflection::EsProxy;
//! use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
//! use es_runtime::esvaluefacade::EsValueFacade;
//! fn test_es_proxy(){
//!
//!     let rt = EsRuntimeWrapperBuilder::new().build();
//!
//!     let proxy: EsProxy = EsProxy::builder(vec!["com", "my", "biz"], "MyClass")
//!     .constructor(|args| {
//!         Ok(1)
//!     })
//!     .finalizer(|obj_id| {
//!         println!("obj {} was garbage collected", obj_id);
//!     })
//!     .method("do_something", |_obj_id, _args| {
//!          println!("doing something in rust");
//!          Ok(EsValueFacade::undefined())
//!     })
//!     .property("my_prop", |_obj_id| {
//!          Ok(EsValueFacade::new_i32(137))
//!     }, |_obj_id, val| {
//!          println!("rust prop my_prop set to {}", val.get_i32());
//!          Ok(())
//!     })
//!     .event("EventA")
//!     .event("EventB")
//!     .build(&rt);
//!
//!     rt.eval_sync("let my_instance = new com.my.biz.MyClass(1, 2, 3); my_instance.do_something(); my_instance.my_prop = 541; let a = my_instance.my_prop; my_instance = null;", "es_proxy_example.es").ok().unwrap();
//!
//! }
//!
//! ```
//!
//!
use crate::es_utils::reflection::{get_proxy, ProxyBuilder};
use crate::esruntimewrapper::EsRuntimeWrapper;
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use crate::esvaluefacade::EsValueFacade;
use mozjs::jsval::JSVal;
use std::collections::{HashMap, HashSet};
use std::ptr::replace;

pub type EsProxyConstructor = dyn Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send;
pub type EsProxyMethod = dyn Fn(i32, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send;
pub type EsProxyFinalizer = dyn Fn(i32) -> () + Send;
pub type EsProxyGetter = dyn Fn(i32) -> Result<EsValueFacade, String> + Send;
pub type EsProxySetter = dyn Fn(i32, EsValueFacade) -> Result<(), String> + Send;
/*
pub type EsProxyStaticMethod = dyn Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send;
pub type EsProxyStaticGetter = dyn Fn() -> Result<EsValueFacade, String> + Send;
pub type EsProxyStaticSetter = dyn Fn(EsValueFacade) -> Result<(), String> + Send;
 */

pub struct EsProxy {
    namespace: Vec<&'static str>,
    class_name: &'static str,
}

pub struct EsProxyBuilder {
    pub namespace: Vec<&'static str>,
    pub class_name: &'static str,
    constructor: Option<Box<EsProxyConstructor>>,
    finalizer: Option<Box<EsProxyFinalizer>>,
    methods: HashMap<&'static str, Box<EsProxyMethod>>,
    properties: HashMap<&'static str, (Box<EsProxyGetter>, Box<EsProxySetter>)>,
    events: HashSet<&'static str>,
    /*
    static_properties: HashMap<&'static str, (Box<EsProxyStaticGetter>, Box<EsProxyStaticSetter>)>,
    static_methods: HashMap<&'static str, Box<EsProxyStaticMethod>>,
    static_events: HashSet<&'static str>,*/
}

impl EsProxy {
    pub fn builder(namespace: Vec<&'static str>, class_name: &'static str) -> EsProxyBuilder {
        EsProxyBuilder::new(namespace, class_name)
    }
    pub fn dispatch_event(
        &self,
        rt: &EsRuntimeWrapperInner,
        obj_id: i32,
        event_name: &'static str,
        event_obj: EsValueFacade,
    ) {
        let p_name = self.class_name;
        rt.do_in_es_runtime_thread(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, _global| {
                let proxy = get_proxy(p_name).unwrap();
                let event_obj_value: JSVal = event_obj.to_es_value(cx);
                rooted!(in (cx) let event_obj_root = event_obj_value);
                proxy.dispatch_event(obj_id, event_name, cx, event_obj_root.handle().into());
            })
        })
    }
    pub fn dispatch_static_event(
        &self,
        rt: &EsRuntimeWrapperInner,
        event_name: &'static str,
        event_obj: EsValueFacade,
    ) {
        let p_name = self.class_name;
        rt.do_in_es_runtime_thread(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, _global| {
                let proxy = get_proxy(p_name).unwrap();
                let event_obj_value: JSVal = event_obj.to_es_value(cx);
                rooted!(in (cx) let event_obj_root = event_obj_value);
                proxy.dispatch_static_event(event_name, cx, event_obj_root.handle().into());
            })
        })
    }
    pub fn get_canonical_name(&self) -> String {
        format!("{}.{}", self.namespace.join("."), self.class_name)
    }
}

impl EsProxyBuilder {
    pub fn new(namespace: Vec<&'static str>, class_name: &'static str) -> Self {
        // todo, this needs it's own members with + Send trait, on build we construct a builder in the worker thread of the runtime
        EsProxyBuilder {
            namespace,
            class_name,
            constructor: None,
            finalizer: None,
            methods: Default::default(),
            properties: Default::default(),
            events: Default::default(),
            /*
            static_properties: Default::default(),
            static_methods: Default::default(),
            static_events: Default::default(),*/
        }
    }
    pub fn constructor<C>(&mut self, constructor: C) -> &mut Self
    where
        C: Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send + 'static,
    {
        self.constructor = Some(Box::new(constructor));
        self
    }
    pub fn finalizer<F>(&mut self, finalizer: F) -> &mut Self
    where
        F: Fn(i32) + Send + 'static,
    {
        self.finalizer = Some(Box::new(finalizer));
        self
    }
    pub fn method<M>(&mut self, name: &'static str, method: M) -> &mut Self
    where
        M: Fn(i32, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + 'static,
    {
        self.methods.insert(name, Box::new(method));
        self
    }
    pub fn property<G, S>(&mut self, name: &'static str, getter: G, setter: S) -> &mut Self
    where
        G: Fn(i32) -> Result<EsValueFacade, String> + Send + 'static,
        S: Fn(i32, EsValueFacade) -> Result<(), String> + Send + 'static,
    {
        self.properties
            .insert(name, (Box::new(getter), Box::new(setter)));
        self
    }

    /// add an event to the EsProxy class
    /// this enables the scripter to add EventHandlers to the instance of your class
    /// # Example
    /// ```javascript
    /// let obj = new my.bix.MyClass();
    /// obj.addEventListener("my_event_type", (evt_obj) => {console.log('event was invoked');});
    /// ```
    ///
    /// from rust you can dispatch events by calling the EsProxy::dispatch_event() method
    pub fn event(&mut self, event_type: &'static str) -> &mut Self {
        self.events.insert(event_type);
        self
    }

    pub fn build(&mut self, rt: &EsRuntimeWrapper) -> EsProxy {
        let cn = self.class_name;
        let ns = self.namespace.clone();
        let constructor_opt = unsafe { replace(&mut self.constructor, None) };
        let finalizer_opt = unsafe { replace(&mut self.finalizer, None) };
        let mut methods = HashMap::new();
        self.methods.drain().all(|entry| {
            methods.insert(entry.0, entry.1);
            true
        });
        let mut properties = HashMap::new();
        self.properties.drain().all(|entry| {
            properties.insert(entry.0, entry.1);
            true
        });
        let events = self.events.clone();

        rt.do_in_es_runtime_thread_sync(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, global| {
                let mut builder = ProxyBuilder::new(ns, cn);

                if let Some(c) = constructor_opt {
                    builder.constructor(move |_cx: *mut mozjs::jsapi::JSContext, args| {
                        crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                            let sm_rt = &*sm_rt_rc.borrow();
                            sm_rt.do_with_jsapi(|rt, cx, global| {
                                let mut es_args: Vec<EsValueFacade> = vec![];
                                for arg_val in args {
                                    let esvf = EsValueFacade::new_v(rt, cx, global, arg_val);
                                    es_args.push(esvf);
                                }
                                c(es_args)
                            })
                        })
                    });
                }
                if let Some(f) = finalizer_opt {
                    builder.finalizer(f);
                }

                methods.drain().all(|method_entry| {
                    let es_method_name = method_entry.0;

                    let es_method = method_entry.1;
                    builder.method(es_method_name, move |_cx, obj_id, args| {
                        crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                            let sm_rt = &*sm_rt_rc.borrow();
                            sm_rt.do_with_jsapi(|rt, cx, global| {
                                let mut es_args: Vec<EsValueFacade> = vec![];
                                for arg_val in args {
                                    let esvf = EsValueFacade::new_v(rt, cx, global, arg_val);
                                    es_args.push(esvf);
                                }

                                let res = es_method(obj_id, es_args);
                                match res {
                                    Ok(esvf) => Ok(esvf.to_es_value(cx)),
                                    Err(err_str) => Err(err_str),
                                }
                            })
                        })
                    });
                    true
                });

                properties.drain().all(|method_entry| {
                    let es_prop_name = method_entry.0;

                    let (es_getter, es_setter) = method_entry.1;
                    builder.property(
                        es_prop_name,
                        move |_cx, obj_id| {
                            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                                let sm_rt = &*sm_rt_rc.borrow();
                                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                                    let res = es_getter(obj_id);
                                    match res {
                                        Ok(esvf) => Ok(esvf.to_es_value(cx)),
                                        Err(err_str) => Err(err_str),
                                    }
                                })
                            })
                        },
                        move |_cx, obj_id, val| {
                            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                                let sm_rt = &*sm_rt_rc.borrow();
                                sm_rt.do_with_jsapi(|rt, cx, global| {
                                    let es_val = EsValueFacade::new_v(rt, cx, global, val);
                                    es_setter(obj_id, es_val)
                                })
                            })
                        },
                    );
                    true
                });

                for evt in events {
                    builder.event(evt);
                }

                let _proxy = builder.build(cx, global);
            });
        });
        EsProxy {
            namespace: self.namespace.clone(),
            class_name: self.class_name,
        }
    }
    pub fn get_canonical_name(&self) -> String {
        format!("{}.{}", self.namespace.join("."), self.class_name)
    }
}
