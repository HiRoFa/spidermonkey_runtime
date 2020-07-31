//! # EsProxy
//!
//! the EsProxy struct provides a simple way to reflect a rust object in the script engine
//!
//! # Example
//!
//! ```no_run
//!
//! use es_runtime::esreflection::EsProxy;
//! use es_runtime::esruntimebuilder::EsRuntimeBuilder;
//! use es_runtime::esvaluefacade::EsValueFacade;
//! use std::collections::HashMap;
//!
//!     let rt = EsRuntimeBuilder::new().build();
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
//!     rt.eval_sync("let my_instance = new com.my.biz.MyClass(1, 2, 3); my_instance.do_something(); my_instance.my_prop = 541; let a = my_instance.my_prop; my_instance = null;", "es_proxy_example.es").ok().expect("script failed");
//!
//!     // create a static class
//!
//!     let static_proxy: EsProxy = EsProxy::builder(vec!["com", "my", "biz"], "MyApp")
//!     .static_method("inform", |args| {
//!          Ok(EsValueFacade::new_bool(true))
//!      })
//!     .static_event("epiphany")
//!     .build(&rt);
//!
//!     rt.eval_sync("com.my.biz.MyApp.addEventListener('epiphany', (evt) => {console.log('Rust had an epiphany about %s', evt.subject);});com.my.biz.MyApp.inform(1, 2, 3);", "es_proxy_example2.es").ok().unwrap();
//!
//!     let mut evt_props = HashMap::new();
//!     evt_props.insert("subject".to_string(), EsValueFacade::new_str("Putting people on Jupiter".to_string()));
//!     let evt_obj = EsValueFacade::new_obj(evt_props);
//!     static_proxy.dispatch_static_event(&rt, "epiphany", evt_obj);
//! ```
//!
//!
use crate::esruntime::EsRuntime;
use crate::esvaluefacade::EsValueFacade;
use crate::jsapi_utils::reflection::{get_proxy, ProxyBuilder};
use mozjs::jsval::JSVal;
use std::collections::{HashMap, HashSet};
use std::ptr::replace;

pub type EsProxyConstructor = dyn Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send;
pub type EsProxyMethod = dyn Fn(&i32, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send;
pub type EsProxyFinalizer = dyn Fn(i32) + Send;
pub type EsProxyGetter = dyn Fn(&i32) -> Result<EsValueFacade, String> + Send;
pub type EsProxySetter = dyn Fn(&i32, EsValueFacade) -> Result<(), String> + Send;
pub type EsProxyStaticMethod = dyn Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send;
pub type EsProxyStaticGetter = dyn Fn() -> Result<EsValueFacade, String> + Send;
pub type EsProxyStaticSetter = dyn Fn(EsValueFacade) -> Result<(), String> + Send;

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

    static_properties: HashMap<&'static str, (Box<EsProxyStaticGetter>, Box<EsProxyStaticSetter>)>,
    static_methods: HashMap<&'static str, Box<EsProxyStaticMethod>>,
    static_events: HashSet<&'static str>,
}

impl EsProxy {
    /// create a builder struct to build an EsProxy
    pub fn builder(namespace: Vec<&'static str>, class_name: &'static str) -> EsProxyBuilder {
        EsProxyBuilder::new(namespace, class_name)
    }
    // todo do we want sync variants which may return a veto boolean or an altered EsValueFacade?
    /// dispatch an event for an instance of the class
    ///
    /// you can pass an EsValueFacade as event obj
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///.constructor(|args| {
    ///    Ok(1)
    ///})
    ///.event("some_event").build(&rt);
    ///rt.eval_sync("let i = new my.biz.MyClass(); \
    ///              i.addEventListener('some_event', (evtObj) => {\
    ///                  console.log('it happened!');\
    ///              });", "test_dispatch_event.es");
    ///es_proxy.dispatch_event(&rt, 1, "some_event", EsValueFacade::undefined());
    /// ```
    pub fn dispatch_event(
        &self,
        rt: &EsRuntime,
        obj_id: i32,
        event_name: &'static str,
        event_obj: EsValueFacade,
    ) {
        let p_name = self.get_canonical_name();
        rt.do_in_es_event_queue(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, _global| {
                let proxy = get_proxy(p_name.as_str()).unwrap();
                let event_obj_value: JSVal = event_obj.to_es_value(cx);
                rooted!(in (cx) let event_obj_root = event_obj_value);
                proxy.dispatch_event(obj_id, event_name, cx, event_obj_root.handle().into());
            });
        });
    }

    /// dispatch an event for an instance of the class
    ///
    /// you can pass an EsValueFacade as event obj
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///.constructor(|args| {
    ///    Ok(1)
    ///})
    ///.event("some_event").build(&rt);
    ///rt.eval_sync("let i = new my.biz.MyClass(); \
    ///              i.addEventListener('some_event', (evtObj) => {\
    ///                  console.log('it happened!');\
    ///              });", "test_dispatch_event.es");
    ///es_proxy.dispatch_event(&rt, 1, "some_event", EsValueFacade::undefined());
    /// ```
    pub fn dispatch_static_event(
        &self,
        rt: &EsRuntime,
        event_name: &'static str,
        event_obj: EsValueFacade,
    ) {
        let p_name = self.get_canonical_name();
        rt.do_in_es_event_queue(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, _global| {
                let proxy = get_proxy(p_name.as_str()).unwrap();
                let event_obj_value: JSVal = event_obj.to_es_value(cx);
                rooted!(in (cx) let event_obj_root = event_obj_value);
                proxy.dispatch_static_event(event_name, cx, event_obj_root.handle().into());
            });
        });
    }

    /// get the canonical name of the Proxy Class, this includes the namespace
    /// e.g. "my.biz.MyApp"
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass").build(&rt);
    ///assert_eq!(es_proxy.get_canonical_name().as_str(), "my.biz.MyClass");
    /// ```
    pub fn get_canonical_name(&self) -> String {
        if self.namespace.is_empty() {
            self.class_name.to_string()
        } else {
            format!("{}.{}", self.namespace.join("."), self.class_name)
        }
    }
}

impl EsProxyBuilder {
    /// create a new EsProxyBuilder
    /// you can pass a namespace as a Vec and a classname as str
    /// if you want to create the class in the global scope you can pass an empty vec as namespace
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass").build(&rt);
    /// ```
    pub fn new(namespace: Vec<&'static str>, class_name: &'static str) -> Self {
        EsProxyBuilder {
            namespace,
            class_name,
            constructor: None,
            finalizer: None,
            methods: Default::default(),
            properties: Default::default(),
            events: Default::default(),
            static_properties: Default::default(),
            static_methods: Default::default(),
            static_events: Default::default(),
        }
    }

    /// the constrcutor is called when the script runtime instantiates an instance of you class
    /// if you do not define a constrcutor for you proxy your proxy will not be constructable
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .constructor(|args| {
    ///         println!("create a new instance");
    ///         // return an id which you can use to identify your rust objects
    ///         Ok(123)
    ///    })
    ///    .build(&rt);
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("let mc = new my.biz.MyClass();", "test_constructor.es").ok().unwrap();
    /// // call the gc
    /// rt.cleanup_sync();
    /// ```
    ///
    pub fn constructor<C>(&mut self, constructor: C) -> &mut Self
    where
        C: Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send + 'static,
    {
        self.constructor = Some(Box::new(constructor));
        self
    }

    /// the finalizer is the opposite of the constrcutor, it is called when the instance
    /// of your class is garbage collected, it is a wayt for you to clean up after
    /// the garbage collector
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .finalizer(|obj_id| {
    ///         println!("do cleanup for objId {}", obj_id);
    ///    })
    ///    .build(&rt);
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("let mc = new my.biz.MyClass(); mc = null;", "test_finalizer.es")
    ///     .ok().expect("script failed");
    /// // call the gc
    /// rt.cleanup_sync();
    /// ```
    ///
    pub fn finalizer<F>(&mut self, finalizer: F) -> &mut Self
    where
        F: Fn(i32) + Send + 'static,
    {
        self.finalizer = Some(Box::new(finalizer));
        self
    }

    /// add a method to the proxy class, the method can be called on an instance of the class
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .constructor(|args| {
    ///         Ok(1)
    ///    })
    ///    .method("doSomething", |obj_id, args| {
    ///         println!("doing something for objId {}", obj_id);
    ///         Ok(EsValueFacade::undefined())
    ///    })
    ///    .build(&rt);
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("let mc = new my.biz.MyClass(); mc.doSomething();", "test_method.es")
    ///     .ok().expect("script failed");
    /// ```
    ///
    pub fn method<M>(&mut self, name: &'static str, method: M) -> &mut Self
    where
        M: Fn(&i32, Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + 'static,
    {
        self.methods.insert(name, Box::new(method));
        self
    }

    /// add a property to the proxy class, the getter and setter can be called on an instance
    /// of the class
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .constructor(|args| {
    ///         Ok(1)
    ///    })
    ///    .property("someProp", |obj_id| {
    ///         println!("getting some_prop for objId {}", obj_id);
    ///         Ok(EsValueFacade::new_i32(1234))
    ///    }, |obj_id, arg| {
    ///         println!("setting some_prop to {} for objId {}", arg.get_i32(), obj_id);             
    ///         Ok(())
    ///     })
    ///    .build(&rt);
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("let mc = new my.biz.MyClass(); \
    /// mc.someProp = 4321; \
    /// console.log('someprop = %s', mc.someProp);"
    /// , "test_property.es").ok().expect("script failed");
    /// ```
    ///
    pub fn property<G, S>(&mut self, name: &'static str, getter: G, setter: S) -> &mut Self
    where
        G: Fn(&i32) -> Result<EsValueFacade, String> + Send + 'static,
        S: Fn(&i32, EsValueFacade) -> Result<(), String> + Send + 'static,
    {
        self.properties
            .insert(name, (Box::new(getter), Box::new(setter)));
        self
    }

    /// define an event type to the proxy class, the event can be dispatched on an instance
    /// of the class
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .constructor(|args| {
    ///          // use id one as obj id
    ///          Ok(1)
    ///    })
    ///    .event("itHappened")
    ///    .build(&rt);
    ///
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("let mc = new my.biz.MyClass(); \
    /// mc.addEventListener('itHappened', \
    ///     (evtObj) => {console.log('Jup, it happened with %s', evtObj);})"
    /// , "test_event.es").ok().expect("script failed");
    ///
    /// // we can then dispatch the event from rust
    /// es_proxy.dispatch_event(&rt, 1, "itHappened", EsValueFacade::new_i32(123));
    /// ```
    ///
    pub fn event(&mut self, event_type: &'static str) -> &mut Self {
        self.events.insert(event_type);
        self
    }

    /// define a static event type to the proxy class, the event can be dispatched directly on the
    /// class without creating an instance
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .static_event("itHappened")
    ///    .build(&rt);
    ///
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("my.biz.MyClass.addEventListener('itHappened', (evtObj) => {console.log('Jup, it happened with %s', evtObj);})", "test_static_event.es").ok().expect("script failed");
    ///
    /// // we can then dispatch the event from rust
    /// es_proxy.dispatch_static_event(&rt, "itHappened", EsValueFacade::new_i32(123));
    /// ```
    ///
    pub fn static_event(&mut self, event_type: &'static str) -> &mut Self {
        self.static_events.insert(event_type);
        self
    }

    /// add a static property to the proxy class, the getter and setter can be called directly on
    /// the class without creating an instance
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .static_property("someProp", || {
    ///         println!("getting some_prop");
    ///         Ok(EsValueFacade::new_i32(1234))
    ///    }, |arg| {
    ///         println!("setting some_prop to {}", arg.get_i32());             
    ///         Ok(())
    ///     })
    ///    .build(&rt);
    /// // we can then eval script which uses the static getter and setter
    /// rt.eval_sync("my.biz.MyClass.someProp = 4321; \
    /// console.log('someprop = %s', my.biz.MyClass.someProp);", "test_static_property.es")
    /// .ok().expect("script failed");
    /// ```
    ///
    pub fn static_property<G, S>(&mut self, name: &'static str, getter: G, setter: S) -> &mut Self
    where
        G: Fn() -> Result<EsValueFacade, String> + Send + 'static,
        S: Fn(EsValueFacade) -> Result<(), String> + Send + 'static,
    {
        self.static_properties
            .insert(name, (Box::new(getter), Box::new(setter)));
        self
    }

    /// add a static method to the proxy class, this can be called directly on the class without
    /// creating an instance
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esreflection::EsProxyBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    ///let rt = EsRuntimeBuilder::default().build();
    ///let es_proxy = EsProxyBuilder::new(vec!["my", "biz"], "MyClass")
    ///    .static_method("doSomethingStatic", |_args| {
    ///        println!("did something static");
    ///        Ok(EsValueFacade::undefined())
    ///    })
    ///    .build(&rt);
    /// // we can then eval script which uses the static method
    /// rt.eval_sync("my.biz.MyClass.doSomethingStatic();", "test_static_method.es")
    /// .ok().expect("script failed");
    /// ```
    ///
    pub fn static_method<M>(&mut self, name: &'static str, method: M) -> &mut Self
    where
        M: Fn(Vec<EsValueFacade>) -> Result<EsValueFacade, String> + Send + 'static,
    {
        self.static_methods.insert(name, Box::new(method));
        self
    }

    /// build the EsProxy this adds the proxy class to the runtime and return an EsProxy object
    pub fn build(&mut self, rt: &EsRuntime) -> EsProxy {
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

        // static
        let mut static_methods = HashMap::new();

        self.static_methods.drain().all(|entry| {
            static_methods.insert(entry.0, entry.1);
            true
        });

        let mut static_properties = HashMap::new();
        self.static_properties.drain().all(|entry| {
            static_properties.insert(entry.0, entry.1);
            true
        });

        let static_events = self.static_events.clone();
        // / static

        rt.do_in_es_event_queue_sync(move |sm_rt| {
            sm_rt.do_with_jsapi(move |_rt, cx, global| {
                let mut builder = ProxyBuilder::new(ns, cn);

                if let Some(c) = constructor_opt {
                    builder.constructor(move |cx: *mut mozjs::jsapi::JSContext, args| {
                        let mut es_args: Vec<EsValueFacade> = vec![];
                        for arg_val in args {
                            let esvf = EsValueFacade::new_v(cx, arg_val);
                            es_args.push(esvf);
                        }
                        c(es_args)
                    });
                }
                if let Some(f) = finalizer_opt {
                    builder.finalizer(f);
                }

                methods.drain().all(|method_entry| {
                    let es_method_name = method_entry.0;

                    let es_method = method_entry.1;
                    builder.method(es_method_name, move |cx, obj_id, args, mut rval| {
                        let mut es_args: Vec<EsValueFacade> = vec![];
                        for arg_val in args {
                            let esvf = EsValueFacade::new_v(cx, arg_val);
                            es_args.push(esvf);
                        }

                        let res = es_method(&obj_id, es_args);
                        match res {
                            Ok(esvf) => {
                                rval.set(esvf.to_es_value(cx));
                                Ok(())
                            }
                            Err(err_str) => Err(err_str),
                        }
                    });
                    true
                });

                properties.drain().all(|method_entry| {
                    let es_prop_name = method_entry.0;

                    let (es_getter, es_setter) = method_entry.1;
                    builder.property(
                        es_prop_name,
                        move |_cx, obj_id, mut rval| {
                            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                                let sm_rt = &*sm_rt_rc.borrow();
                                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                                    let res = es_getter(&obj_id);
                                    match res {
                                        Ok(esvf) => {
                                            rval.set(esvf.to_es_value(cx));
                                            Ok(())
                                        }
                                        Err(err_str) => Err(err_str),
                                    }
                                })
                            })
                        },
                        move |cx, obj_id, val| {
                            let es_val = EsValueFacade::new_v(cx, val);
                            es_setter(&obj_id, es_val)
                        },
                    );
                    true
                });

                for evt in events {
                    builder.event(evt);
                }

                static_methods.drain().all(|method_entry| {
                    let es_method_name = method_entry.0;

                    let es_method = method_entry.1;
                    builder.static_method(es_method_name, move |cx, args, mut rval| {
                        let mut es_args: Vec<EsValueFacade> = vec![];
                        for arg_val in args {
                            let esvf = EsValueFacade::new_v(cx, arg_val);
                            es_args.push(esvf);
                        }

                        let res = es_method(es_args);
                        match res {
                            Ok(esvf) => {
                                rval.set(esvf.to_es_value(cx));
                                Ok(())
                            }
                            Err(err_str) => Err(err_str),
                        }
                    });
                    true
                });

                static_properties.drain().all(|method_entry| {
                    let es_prop_name = method_entry.0;

                    let (es_getter, es_setter) = method_entry.1;
                    builder.static_property(
                        es_prop_name,
                        move |_cx, mut rval| {
                            crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                                let sm_rt = &*sm_rt_rc.borrow();
                                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                                    let res = es_getter();
                                    match res {
                                        Ok(esvf) => {
                                            rval.set(esvf.to_es_value(cx));
                                            Ok(())
                                        }
                                        Err(err_str) => Err(err_str),
                                    }
                                })
                            })
                        },
                        move |cx, val| {
                            let es_val = EsValueFacade::new_v(cx, val);
                            es_setter(es_val)
                        },
                    );
                    true
                });

                for evt in static_events {
                    builder.static_event(evt);
                }

                let _proxy = builder.build(cx, global);
            });
        });
        EsProxy {
            namespace: self.namespace.clone(),
            class_name: self.class_name,
        }
    }

    /// get the canonical name of the proxy class, this includes the namespace
    /// e.g. "my.biz.MyApp"
    pub fn get_canonical_name(&self) -> String {
        if self.namespace.is_empty() {
            self.class_name.to_string()
        } else {
            format!("{}.{}", self.namespace.join("."), self.class_name)
        }
    }
}
