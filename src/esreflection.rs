use crate::es_utils::reflection::{get_proxy, ProxyBuilder};
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use crate::esvaluefacade::EsValueFacade;
use mozjs::jsval::JSVal;
use mozjs::rust::HandleValue;
use std::collections::{HashMap, HashSet};

pub struct EsProxy {
    proxy_name: &'static str,
}

pub struct EsProxyBuilder {
    pub class_name: &'static str,
    constructor: Option<Box<dyn Fn(Vec<EsValueFacade>) -> Result<i32, String> + Send>>,
    finalizer: Option<Box<dyn Fn(&i32) -> () + Send>>,
    properties: HashMap<
        &'static str,
        (
            Box<dyn Fn(i32) -> EsValueFacade + Send>,
            Box<dyn Fn(i32, EsValueFacade) -> () + Send>,
        ),
    >,
    methods: HashMap<&'static str, Box<dyn Fn(i32, Vec<EsValueFacade>) -> EsValueFacade + Send>>,
    events: HashSet<&'static str>,
    static_properties: HashMap<
        &'static str,
        (
            Box<dyn Fn() -> EsValueFacade + Send>,
            Box<dyn Fn(EsValueFacade) -> () + Send>,
        ),
    >,
    static_methods: HashMap<&'static str, Box<dyn Fn(Vec<EsValueFacade>) -> EsValueFacade + Send>>,
    static_events: HashSet<&'static str>,
}

impl EsProxy {
    pub fn dispatch_event(
        &self,
        rt: &EsRuntimeWrapperInner,
        obj_id: i32,
        event_name: &'static str,
        event_obj: EsValueFacade,
    ) {
        let p_name = self.proxy_name;
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
        let p_name = self.proxy_name;
        rt.do_in_es_runtime_thread(move |sm_rt| {
            sm_rt.do_with_jsapi(move |rt, cx, global| {
                let proxy = get_proxy(p_name).unwrap();
                let event_obj_value: JSVal = event_obj.to_es_value(cx);
                rooted!(in (cx) let event_obj_root = event_obj_value);
                proxy.dispatch_static_event(event_name, cx, event_obj_root.handle().into());
            })
        })
    }
}

impl EsProxyBuilder {
    pub fn new(class_name: &'static str) -> Self {
        // todo, this needs it's own members with + Send trait, on build we construct a builder in the worker thread of the runtime
        EsProxyBuilder {
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
    pub fn build(&mut self, rt: &EsRuntimeWrapperInner) -> EsProxy {
        let cn = self.class_name;

        rt.do_in_es_runtime_thread_sync(move |sm_rt| {
            sm_rt.do_with_jsapi(move |rt, cx, global| {
                let mut proxy_builder = ProxyBuilder::new(cn);

                let _proxy = proxy_builder.build(cx, global);
            });
        });
        EsProxy {
            proxy_name: self.class_name,
        }
    }
}
