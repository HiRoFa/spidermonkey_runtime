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
//! fn test_es_proxy(){
//!
//!     let rt = EsRuntimeWrapperBuilder::new().build();
//!     
//!     let proxy: EsProxy = EsProxy::builder(vec!["com", "my", "biz"], "MyClass")
//!     .constructor(|args| {
//!         Ok(1)
//!     }).build(&rt);
//!
//!     rt.eval_sync("let my_instance = new com.my.biz.MyClass(1, 2, 3);", "esproxy_example.es");
//!
//! }
//!
//! ```
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
pub type EsProxyGetter = dyn Fn(i32) -> Result<EsValueFacade, String> + Send;
pub type EsProxySetter = dyn Fn(i32, EsValueFacade) -> Result<(), String> + Send;
pub type EsProxyFinalizer = dyn Fn(&i32) -> () + Send;
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
    properties: HashMap<&'static str, (Box<EsProxyGetter>, Box<EsProxySetter>)>,

    methods: HashMap<&'static str, Box<EsProxyMethod>>,
    events: HashSet<&'static str>,
    static_properties: HashMap<&'static str, (Box<EsProxyStaticGetter>, Box<EsProxyStaticSetter>)>,

    static_methods: HashMap<&'static str, Box<EsProxyStaticMethod>>,
    static_events: HashSet<&'static str>,
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
            sm_rt.do_with_jsapi(move |rt, cx, global| {
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
            sm_rt.do_with_jsapi(move |rt, cx, global| {
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
            properties: Default::default(),
            methods: Default::default(),
            events: Default::default(),
            static_properties: Default::default(),
            static_methods: Default::default(),
            static_events: Default::default(),
        }
    }
    pub fn constructor<C>(&mut self, constructor: C) -> &mut Self
    where
        C: Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send + 'static,
    {
        self.constructor = Some(Box::new(constructor));
        self
    }
    pub fn build(&mut self, rt: &EsRuntimeWrapper) -> EsProxy {
        let cn = self.class_name;
        let ns = self.namespace.clone();
        let mut constructor_opt = unsafe { replace(&mut self.constructor, None) };

        rt.do_in_es_runtime_thread_sync(move |sm_rt| {
            sm_rt.do_with_jsapi(move |rt, cx, global| {
                let mut builder = ProxyBuilder::new(ns, cn);

                if let Some(c) = constructor_opt {
                    builder.constructor(move |cx: *mut mozjs::jsapi::JSContext, args| {
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
