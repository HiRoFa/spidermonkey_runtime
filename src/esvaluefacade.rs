use log::trace;

use crate::esruntime::EsRuntime;
use crate::esruntimeinner::EsRuntimeInner;
use crate::jsapi_utils::arrays::{get_array_element, get_array_length, new_array, object_is_array};
use crate::jsapi_utils::objects::NULL_JSOBJECT;
use crate::jsapi_utils::rooting::EsPersistentRooted;
use crate::jsapi_utils::{objects, EsErrorInfo};
use crate::spidermonkeyruntimewrapper::SmRuntime;
use crate::utils::AutoIdMap;
use crate::{jsapi_utils, spidermonkeyruntimewrapper};
use either::Either;
use hirofa_utils::debug_mutex::DebugMutex;
use log::debug;
use mozjs::jsapi::HandleValueArray;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::{BooleanValue, DoubleValue, Int32Value, JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::{HandleValue, MutableHandleValue};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Weak};
use std::time::Duration;
use hirofa_utils::eventloop::EventLoop;

// placeholder for promises that were passed from the script engine to rust
struct CachedJSPromise {
    cached_obj_id: usize,
    opt_receiver: Option<Receiver<Result<EsValueFacade, EsValueFacade>>>,
    rti_ref: Arc<EsRuntimeInner>,
}

impl Drop for CachedJSPromise {
    fn drop(&mut self) {
        let rt_arc = self.rti_ref.clone();
        let cached_obj_id = self.cached_obj_id;

        rt_arc.do_in_es_event_queue(move |_sm_rt| {
            spidermonkeyruntimewrapper::remove_cached_object(cached_obj_id);
        });
    }
}

// placeholder for functions that were passed from the script engine to rust
struct CachedJSFunction {
    cached_obj_id: usize,
    rti_ref: Arc<EsRuntimeInner>,
}

struct RustPromise {
    id: usize,
}

impl RustPromise {
    fn new_esvf<C>(resolver: C) -> EsValueFacade
    where
        C: FnOnce() -> Result<EsValueFacade, String> + Send + 'static,
    {
        // create a lazy_static map in a Mutex
        // the mutex contains a Map<usize, Either<Result<EsValueFacade, EsErrorInfo>, EsPersistentRooted>>
        // the usize is stored as an id in self.val_promise_id

        //

        // the task is fed to a thread_pool here
        // in the task, when complete
        // see if we have a epr, if so resolve that, if not put answer in left

        // on get_es_val

        // get lock, see if we have an answer already
        // if so create promise and resolve it
        // if not create promise and put in map as EsPersistentRooted

        // on drop of EsValueFacade
        // if map val for key is None, remove from map
        trace!("prepping promise, gen id");

        let id = {
            // locked scope
            let map: &mut PromiseAnswersMap = &mut PROMISE_ANSWERS.lock("gen_id").unwrap();

            map.insert(None)
        }; // end locked scope

        trace!("prepping promise {}", id);

        let task = move || {
            trace!("running prom reso task for {}", id);
            let res = resolver();
            trace!("got prom result for {}, ok={}", id, res.is_ok());
            let either_opt: Option<(PromiseResultContainer, Result<EsValueFacade, String>)> = {
                // locked scope
                let map: &mut PromiseAnswersMap = &mut PROMISE_ANSWERS.lock("in_task").unwrap();

                if map.contains_key(&id) {
                    let val = map.get(&id).unwrap();
                    if val.is_none() {
                        trace!("PROMISE_ANSWERS had Some for {} setting to val", id);
                        // set result in left
                        let new_val = Some(Either::Left(res));
                        map.replace(&id, new_val);
                        None
                    } else {
                        trace!("PROMISE_ANSWERS had Some resolve promise in right");
                        // resolve promise in right
                        // we are in a different thread here
                        // we need a weakref to the runtime here, os we can run in the es thread
                        // will be stored in a tuple with the EsPersisistentRooted

                        let eith = map.remove(&id).unwrap();

                        Some((eith, res))

                        // eith and thus EsPersistentRooted is dropped here
                    }
                } else {
                    // EsValueFacade was dropped before instantiating a promise obj
                    // do nothing
                    trace!("PROMISE_ANSWERS had no val for {}", id);
                    None
                }
            }; // end of locked scope

            if let Some((eith, res)) = either_opt {
                if eith.is_right() {
                    // in our right we have a rooted promise and a weakref to our runtimeinner
                    let (prom_regged_id, weak_rt_ref) = eith.right().unwrap();
                    trace!("found promise with id {} in right", prom_regged_id);

                    let rt_opt = weak_rt_ref.upgrade();
                    if let Some(rti) = rt_opt {
                        rti.do_in_es_event_queue_sync(Box::new(move |sm_rt: &SmRuntime| {
                            // resolve or reject promise
                            sm_rt.do_with_jsapi(move |_rt, cx, _global| {
                                let prom_obj: *mut JSObject = {
                                    let epr = spidermonkeyruntimewrapper::remove_cached_object(
                                        prom_regged_id,
                                    );
                                    epr.get()
                                };
                                trace!("epr should be dropped here");
                                rooted!(in (cx) let mut prom_obj_root = prom_obj);
                                trace!("rooted promise");

                                if res.is_ok() {
                                    trace!("rooting result");
                                    rooted!(in (cx) let mut res_root = UndefinedValue());
                                    res.ok().unwrap().to_es_value(cx, res_root.handle_mut());
                                    trace!("resolving prom");
                                    let resolve_prom_res = jsapi_utils::promises::resolve_promise(
                                        cx,
                                        prom_obj_root.handle(),
                                        res_root.handle(),
                                    );
                                    if resolve_prom_res.is_err() {
                                        panic!(
                                            "could not resolve promise {} because of error: {}",
                                            prom_regged_id,
                                            resolve_prom_res.err().unwrap().err_msg()
                                        );
                                    }
                                } else {
                                    trace!("rooting err result");
                                    let err_str = res.err().unwrap();

                                    rooted!(in (cx) let mut res_root = UndefinedValue());
                                    jsapi_utils::new_es_value_from_str(
                                        cx,
                                        err_str.as_str(),
                                        res_root.handle_mut(),
                                    );

                                    trace!("rejecting prom");
                                    let reject_prom_res = jsapi_utils::promises::reject_promise(
                                        cx,
                                        prom_obj_root.handle(),
                                        res_root.handle(),
                                    );
                                    if reject_prom_res.is_err() {
                                        panic!(
                                            "could not reject promise {} because of error: {}",
                                            prom_regged_id,
                                            reject_prom_res.err().unwrap().err_msg()
                                        );
                                    }
                                }
                            });
                        }));
                    } else {
                        trace!("rt was dropped before getting val for {}", id);
                    }
                } else {
                    // wtf
                    panic!("eith had unexpected left");
                }
            }
        };

        trace!("spawning prom reso task for {}", id);

        // run task
        EsRuntime::add_helper_task(task);

        RustPromise { id }.to_es_value_facade()
    }
}

pub trait EsValueConvertible {
    fn to_js_value(&self, cx: *mut JSContext, return_val: MutableHandleValue);

    fn to_es_value_facade(self) -> EsValueFacade
    where
        Self: Sized + Send + 'static,
    {
        EsValueFacade {
            convertible: Box::new(self),
        }
    }

    fn is_null(&self) -> bool {
        false
    }

    fn is_undefined(&self) -> bool {
        false
    }

    fn is_bool(&self) -> bool {
        false
    }
    fn get_bool(&self) -> bool {
        panic!("i am not a boolean");
    }
    fn is_str(&self) -> bool {
        false
    }
    fn get_str(&self) -> &str {
        panic!("i am not a string");
    }
    fn is_i32(&self) -> bool {
        false
    }
    fn get_i32(&self) -> i32 {
        panic!("i am not an i32");
    }
    fn is_f64(&self) -> bool {
        false
    }
    fn get_f64(&self) -> f64 {
        panic!("i am not an f64");
    }
    fn is_function(&self) -> bool {
        false
    }
    fn invoke_function(&self, _args: Vec<EsValueFacade>) -> Result<EsValueFacade, EsErrorInfo> {
        panic!("i am not a function");
    }
    fn is_promise(&self) -> bool {
        false
    }
    fn await_promise_blocking(
        &self,
        _timeout: Duration,
    ) -> Result<Result<EsValueFacade, EsValueFacade>, RecvTimeoutError> {
        panic!("i am not a promise");
    }
    fn is_object(&self) -> bool {
        false
    }
    fn get_object(&self) -> &HashMap<String, EsValueFacade> {
        panic!("i am not an object");
    }
    fn is_array(&self) -> bool {
        false
    }
    fn get_array(&self) -> &Vec<EsValueFacade> {
        panic!("i am not an array");
    }
}

struct EsUndefinedValue {}

impl EsValueConvertible for EsUndefinedValue {
    fn to_js_value(&self, _cx: *mut JSContext, _rval: MutableHandleValue) {
        //
    }
}

impl EsValueConvertible for CachedJSPromise {
    fn to_js_value(&self, _cx: *mut JSContext, _rval: MutableHandleValue) {
        unimplemented!()
    }

    fn is_promise(&self) -> bool {
        true
    }

    fn await_promise_blocking(
        &self,
        timeout: Duration,
    ) -> Result<Result<EsValueFacade, EsValueFacade>, RecvTimeoutError> {
        if !self.is_promise() {
            return Ok(Err(EsValueFacade::new_str(
                "esvf was not a Promise".to_string(),
            )));
        }

        if EventLoop::is_a_pool_thread() {
            log::error!("waiting for esvf prom from event queue thread, bad dev bad!");
            panic!("you really should not wait for promises in a RT's event queue thread");
        }

        let rx = self.opt_receiver.as_ref().expect("not a waiting promise");
        rx.recv_timeout(timeout)
    }
}

impl EsValueConvertible for RustPromise {
    fn to_js_value(&self, cx: *mut JSContext, rval: MutableHandleValue) {
        let mut rval = rval;
        trace!("to_es_value.7 prepped_promise");
        let map: &mut PromiseAnswersMap = &mut PROMISE_ANSWERS.lock("to_es_value.7").unwrap();
        let id = self.id;
        if let Some(opt) = map.get(&id) {
            trace!("create promise");
            // create promise
            let prom = jsapi_utils::promises::new_promise(cx);
            trace!("rooting promise");
            rooted!(in (cx) let prom_root = prom);

            if opt.is_none() {
                trace!("set rooted Promise obj and weakref in right");
                // set rooted Promise obj and weakref in right

                let (pid, rti_ref) = spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                    let sm_rt: &SmRuntime = &*sm_rt_rc.borrow();

                    let pid = spidermonkeyruntimewrapper::register_cached_object(cx, prom);

                    let weakref = sm_rt.opt_esrt_inner.as_ref().unwrap().clone();

                    (pid, weakref)
                });
                map.replace(&id, Some(Either::Right((pid, rti_ref))));
            } else {
                trace!("remove eith from map and resolve promise with left");
                // remove eith from map and resolve promise with left
                let eith = map.remove(&id).unwrap();

                if eith.is_left() {
                    let res = eith.left().unwrap();
                    if res.is_ok() {
                        rooted!(in (cx) let mut res_root = UndefinedValue());
                        res.ok().unwrap().to_es_value(cx, res_root.handle_mut());
                        let prom_reso_res = jsapi_utils::promises::resolve_promise(
                            cx,
                            prom_root.handle(),
                            res_root.handle(),
                        );
                        if prom_reso_res.is_err() {
                            panic!(
                                "could not resolve promise: {}",
                                prom_reso_res.err().unwrap().err_msg()
                            );
                        }
                    } else {
                        // reject prom
                        let err_str = res.err().unwrap();
                        rooted!(in (cx) let mut res_root = UndefinedValue());
                        jsapi_utils::new_es_value_from_str(
                            cx,
                            err_str.as_str(),
                            res_root.handle_mut(),
                        );

                        let prom_reje_res = jsapi_utils::promises::reject_promise(
                            cx,
                            prom_root.handle(),
                            res_root.handle(),
                        );
                        if prom_reje_res.is_err() {
                            panic!(
                                "could not reject promise: {}",
                                prom_reje_res.err().unwrap().err_msg()
                            );
                        }
                    }
                } else {
                    panic!("eith had unexpected right for id {}", id);
                }
            }
            rval.set(ObjectValue(prom));
        } else {
            panic!("PROMISE_ANSWERS had no val for id {}", id);
        }
    }
}

impl CachedJSFunction {
    fn invoke_function1(&self, args: Vec<EsValueFacade>) -> Result<EsValueFacade, EsErrorInfo> {
        let rt_arc = self.rti_ref.clone();
        let cached_id = self.cached_obj_id;

        let job = move |sm_rt: &SmRuntime| Self::invoke_function2(cached_id, sm_rt, args);

        rt_arc.do_in_es_event_queue_sync(job)
    }

    fn invoke_function2(
        cached_id: usize,
        sm_rt: &SmRuntime,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function2()");
        sm_rt.do_with_jsapi(|_rt, cx, _global| Self::invoke_function3(cached_id, cx, args))
    }

    fn invoke_function3(
        cached_id: usize,
        cx: *mut JSContext,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function3()");
        spidermonkeyruntimewrapper::do_with_cached_object(cached_id, |epr: &EsPersistentRooted| {
            auto_root!(in (cx) let mut args_rooted_vec = vec![]);

            for esvf in &args {
                rooted!(in (cx) let mut arg_val = UndefinedValue());
                esvf.to_es_value(cx, arg_val.handle_mut());
                args_rooted_vec.push(*arg_val);
            }

            let arguments_value_array =
                unsafe { HandleValueArray::from_rooted_slice(&*args_rooted_vec) };

            rooted!(in (cx) let mut rval = UndefinedValue());
            rooted!(in (cx) let scope = NULL_JSOBJECT);
            rooted!(in (cx) let function_val = mozjs::jsval::ObjectValue(epr.get()));

            let res2: Result<(), EsErrorInfo> = jsapi_utils::functions::call_function_value2(
                cx,
                scope.handle(),
                function_val.handle(),
                arguments_value_array,
                rval.handle_mut(),
            );

            if res2.is_ok() {
                Ok(EsValueFacade::new_v(cx, rval.handle()))
            } else {
                Err(res2.err().unwrap())
            }
        })
    }
}

impl EsValueConvertible for CachedJSFunction {
    fn to_js_value(&self, _cx: *mut JSContext, _rval: MutableHandleValue) {
        unimplemented!()
    }

    fn is_function(&self) -> bool {
        true
    }

    fn invoke_function(&self, args: Vec<EsValueFacade>) -> Result<EsValueFacade, EsErrorInfo> {
        self.invoke_function1(args)
    }
}

impl EsValueConvertible for String {
    fn to_js_value(&self, cx: *mut JSContext, rval: MutableHandleValue) {
        jsapi_utils::new_es_value_from_str(cx, self.as_str(), rval);
    }

    fn is_str(&self) -> bool {
        true
    }

    fn get_str(&self) -> &str {
        self.as_str()
    }
}

impl EsValueConvertible for i32 {
    fn to_js_value(&self, _cx: *mut JSContext, rval: MutableHandleValue) {
        let mut rval = rval;
        rval.set(Int32Value(*self))
    }

    fn is_i32(&self) -> bool {
        true
    }

    fn get_i32(&self) -> i32 {
        *self
    }
}

impl EsValueConvertible for bool {
    fn to_js_value(&self, _cx: *mut JSContext, rval: MutableHandleValue) {
        let mut rval = rval;
        rval.set(BooleanValue(*self))
    }
    fn is_bool(&self) -> bool {
        true
    }

    fn get_bool(&self) -> bool {
        *self
    }
}

impl EsValueConvertible for f64 {
    fn to_js_value(&self, _cx: *mut JSContext, rval: MutableHandleValue) {
        let mut rval = rval;
        rval.set(DoubleValue(*self))
    }
    fn is_f64(&self) -> bool {
        true
    }

    fn get_f64(&self) -> f64 {
        *self
    }
}

impl EsValueConvertible for Vec<EsValueFacade> {
    fn to_js_value(&self, cx: *mut JSContext, rval: MutableHandleValue) {
        rooted!(in (cx) let mut arr_root = NULL_JSOBJECT);
        // create the array
        new_array(cx, arr_root.handle_mut());
        // add items
        for item in self {
            rooted!(in (cx) let mut arr_elem_val = UndefinedValue());
            // convert elem to JSVal
            item.to_es_value(cx, arr_elem_val.handle_mut());
            // add to array
            jsapi_utils::arrays::push_array_element(cx, arr_root.handle(), arr_elem_val.handle())
                .ok()
                .expect("jsapi_utils::arrays::push_array_element failed");
        }
        let mut rval = rval;
        rval.set(ObjectValue(*arr_root));
    }

    fn is_array(&self) -> bool {
        true
    }

    fn get_array(&self) -> &Vec<EsValueFacade> {
        self
    }
}

impl EsValueConvertible for HashMap<String, EsValueFacade> {
    fn to_js_value(&self, cx: *mut JSContext, rval: MutableHandleValue) {
        trace!("to_es_value.6");
        rooted!(in(cx) let mut obj_root = NULL_JSOBJECT);
        jsapi_utils::objects::new_object(cx, obj_root.handle_mut());

        for prop in self {
            let prop_name = prop.0;
            let prop_esvf = prop.1;
            rooted!(in(cx) let mut val_root = UndefinedValue());
            prop_esvf.to_es_value(cx, val_root.handle_mut());
            jsapi_utils::objects::set_es_obj_prop_value(
                cx,
                obj_root.handle(),
                prop_name,
                val_root.handle(),
            );
        }
        let mut rval = rval;
        rval.set(ObjectValue(*obj_root));
    }

    fn is_object(&self) -> bool {
        true
    }

    fn get_object(&self) -> &HashMap<String, EsValueFacade, RandomState> {
        self
    }
}

/// the EsValueFacade is a converter between rust variables and script objects
/// when receiving a EsValueFacade from the script engine it's data is always a clone from the actual data so we need not worry about the value being garbage collected
///
/// # Example
///
/// ```no_run
/// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
///
/// let rt = EsRuntimeBuilder::default().build();
/// let esvf = rt.eval_sync("123", "test_es_value_facade.es").ok().unwrap();
/// assert!(esvf.is_i32());
/// assert_eq!(esvf.get_i32(), 123);
/// ```
pub struct EsValueFacade {
    convertible: Box<dyn EsValueConvertible + Send>,
}

type PromiseAnswersMap = AutoIdMap<PromiseResultContainerOption>;

lazy_static! {
    static ref PROMISE_ANSWERS: Arc<DebugMutex<PromiseAnswersMap>> =
        Arc::new(DebugMutex::new(AutoIdMap::new(), "PROMISE_ANSWERS"));
}

impl EsValueFacade {
    /// create a new EsValueFacade representing an undefined value
    pub fn undefined() -> Self {
        EsUndefinedValue {}.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a float
    pub fn new_f64(num: f64) -> Self {
        num.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a basic object with properties as defined in the HashMap
    pub fn new_obj(props: HashMap<String, EsValueFacade>) -> Self {
        props.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a signed integer
    pub fn new_i32(num: i32) -> Self {
        num.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a String
    pub fn new_str(s: String) -> Self {
        s.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a bool
    pub fn new_bool(b: bool) -> Self {
        b.to_es_value_facade()
    }

    /// create a new EsValueFacade representing an Array
    pub fn new_array(vals: Vec<EsValueFacade>) -> Self {
        vals.to_es_value_facade()
    }

    /// create a new EsValueFacade representing a Promise, the passed closure will actually run in a seperate helper thread and resolve the Promise that is created in the script runtime
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    /// use std::time::Duration;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// rt.eval_sync("let myFunc = function(a){\
    ///     a.then((res) => {\
    ///         console.log('a resolved with %s', res);\
    ///     });\
    /// };", "test_new_promise.es");
    /// let esvf_arg = EsValueFacade::new_promise(|| {
    ///     // do complicated calculations or whatever here, it will run async
    ///     // then return Ok to resolve the promise or Err to reject it
    ///     Ok(EsValueFacade::new_i32(123))
    /// });
    /// rt.call_sync(vec![], "myFunc", vec![esvf_arg]);
    /// // wait for promise to resolve
    /// std::thread::sleep(Duration::from_secs(1));
    /// ```
    pub fn new_promise<C>(resolver: C) -> EsValueFacade
    where
        C: FnOnce() -> Result<EsValueFacade, String> + Send + 'static,
    {
        RustPromise::new_esvf(resolver)
    }

    pub(crate) fn new_v(context: *mut JSContext, val_handle: HandleValue) -> Self {
        let val: JSVal = *val_handle;

        trace!("EsValueFacade::new_v");

        if val.is_boolean() {
            trace!("EsValueFacade::new_v -> boolean");
            val.to_boolean().to_es_value_facade()
        } else if val.is_int32() {
            trace!("EsValueFacade::new_v -> int32");
            val.to_int32().to_es_value_facade()
        } else if val.is_double() {
            trace!("EsValueFacade::new_v -> double");
            val.to_number().to_es_value_facade()
        } else if val.is_string() {
            trace!("EsValueFacade::new_v -> string");
            jsapi_utils::es_value_to_str(context, val)
                .expect("could not convert jsval to string")
                .to_es_value_facade()
        } else if val.is_object() {
            trace!("EsValueFacade::new_v -> object");
            let obj: *mut JSObject = val.to_object();
            Self::new_v_from_object(context, obj)
        } else if val.is_null() {
            trace!("EsValueFacade::new_v -> null");
            // todo impl EsNull
            EsUndefinedValue {}.to_es_value_facade()
        } else if val.is_undefined() {
            trace!("EsValueFacade::new_v -> undefined");
            EsUndefinedValue {}.to_es_value_facade()
        } else {
            trace!("EsValueFacade::new_v -> unknown");
            EsUndefinedValue {}.to_es_value_facade()
        }
    }

    fn new_v_from_object(context: *mut JSContext, obj: *mut JSObject) -> Self {
        rooted!(in(context) let obj_root = obj);

        if object_is_array(context, obj_root.handle()) {
            trace!("EsValueFacade::new_v -> object -> array");
            let mut vals = vec![];
            // add vals

            let arr_len = get_array_length(context, obj_root.handle()).ok().unwrap();
            for x in 0..arr_len {
                rooted!(in (context) let mut arr_element_root = UndefinedValue());
                let get_res =
                    get_array_element(context, obj_root.handle(), x, arr_element_root.handle_mut());
                if get_res.is_err() {
                    panic!(
                        "could not get element of array: {}",
                        get_res.err().unwrap().err_msg()
                    );
                }
                vals.push(EsValueFacade::new_v(context, arr_element_root.handle()));
            }

            vals.to_es_value_facade()
        } else if jsapi_utils::promises::object_is_promise(obj_root.handle()) {
            trace!("EsValueFacade::new_v -> object -> promise");

            let cached_prom_id =
                spidermonkeyruntimewrapper::register_cached_object(context, *obj_root);

            let (tx, rx) = channel();
            let tx2 = tx.clone();
            assert!(jsapi_utils::promises::add_promise_reactions_callbacks(
                context,
                obj_root.handle(),
                Some(
                    move |cx, mut args: Vec<HandleValue>, _rval: MutableHandleValue| {
                        // promsie was resolved
                        let resolution = args.remove(0);
                        let res_esvf = EsValueFacade::new_v(cx, resolution);

                        match tx.send(Ok(res_esvf)) {
                            Ok(_) => Ok(()),
                            // todo, does not include error (which is "sending on a closed channel") which is not ASCII and thus fails the error handler
                            Err(e) => {
                                debug!("send res error: {}", e);
                                Err("send res error".to_string())
                            }
                        }
                    }
                ),
                Some(
                    move |cx, mut args: Vec<HandleValue>, _rval: MutableHandleValue| {
                        // promsie was rejected
                        let rejection = args.remove(0);
                        let rej_esvf = EsValueFacade::new_v(cx, rejection);

                        match tx2.send(Err(rej_esvf)) {
                            Ok(_) => Ok(()),
                            // todo, does not include error (which is "sending on a closed channel") which is not ASCII and thus fails the error handler
                            Err(e) => {
                                debug!("send rejection error: {}", e);
                                Err("send rejection error".to_string())
                            }
                        }
                        // release epr
                    }
                )
            ));

            let opt_receiver = Some(rx);

            let rti_ref = spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                let sm_rt: &SmRuntime = &*sm_rt_rc.borrow();
                sm_rt.clone_esrt_inner()
            });
            let rmev: CachedJSPromise = CachedJSPromise {
                cached_obj_id: cached_prom_id,
                opt_receiver,
                rti_ref,
            };

            rmev.to_es_value_facade()
        } else if jsapi_utils::functions::object_is_function(obj) {
            trace!("EsValueFacade::new_v -> object -> function");
            // wrap function in persistentrooted

            let rti_ref = spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                let sm_rt: &SmRuntime = &*sm_rt_rc.borrow();
                sm_rt.clone_esrt_inner()
            });
            let cached_obj_id = spidermonkeyruntimewrapper::register_cached_object(context, obj);
            let cf = CachedJSFunction {
                cached_obj_id,
                rti_ref,
            };
            cf.to_es_value_facade()
        } else {
            let mut map = HashMap::new();
            trace!("EsValueFacade::new_v -> object -> object");
            let prop_names: Vec<String> =
                objects::get_js_obj_prop_names(context, obj_root.handle());
            for prop_name in prop_names {
                rooted!(in (context) let mut prop_val_root = UndefinedValue());
                let prop_val_res = objects::get_es_obj_prop_val(
                    context,
                    obj_root.handle(),
                    prop_name.as_str(),
                    prop_val_root.handle_mut(),
                );

                if prop_val_res.is_err() {
                    panic!(
                        "error getting prop {}: {}",
                        prop_name,
                        prop_val_res.err().unwrap().err_msg()
                    );
                }

                let prop_esvf = EsValueFacade::new_v(context, prop_val_root.handle());
                map.insert(prop_name, prop_esvf);
            }
            map.to_es_value_facade()
        }
    }

    /// get the String value
    pub fn get_string(&self) -> &str {
        self.convertible.get_str()
    }

    /// get the i32 value
    pub fn get_i32(&self) -> i32 {
        self.convertible.get_i32()
    }

    /// get the f64 value
    pub fn get_f64(&self) -> f64 {
        self.convertible.get_f64()
    }

    /// get the boolean value
    pub fn get_boolean(&self) -> bool {
        self.convertible.get_bool()
    }

    /// check if this esvf was a promise which was returned from the script engine
    pub fn is_promise(&self) -> bool {
        self.convertible.is_promise()
    }

    /// wait for a promise to resolve in rust
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use std::time::Duration;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// // run the script and fail if script fails
    /// let esvf_prom = rt.eval_sync(
    ///     "let p = new Promise((resolve, reject) => {setImmediate(() => {resolve(123);});}); p;",
    ///     "test_get_promise_result_blocking.es").ok().expect("script failed");
    /// // wait for the promise or fail on timeout
    /// let wait_res = esvf_prom.get_promise_result_blocking(Duration::from_secs(1))
    ///     .ok().expect("promise timed out");
    /// // get the ok result, fail is promise was rejected
    /// let esvf = wait_res.ok().expect("promise was rejected");
    /// // check the result
    /// assert_eq!(esvf.get_i32(), 123);
    /// ```
    pub fn get_promise_result_blocking(
        &self,
        timeout: Duration,
    ) -> Result<Result<EsValueFacade, EsValueFacade>, RecvTimeoutError> {
        // todo
        self.convertible.await_promise_blocking(timeout)
    }

    /// get the value as a Map of EsValueFacades, this works when the value was an object in the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// let esvf = rt.eval_sync("{a: 1, b: 2};", "test_get_object.es").ok().expect("script failed");
    /// let map = esvf.get_object();
    /// assert!(map.contains_key("a"));
    /// assert!(map.contains_key("b"));
    /// ```
    pub fn get_object(&self) -> &HashMap<String, EsValueFacade> {
        self.convertible.get_object()
    }

    /// get the value as a Vec of EsValueFacades, this works when the value was an array in the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// let esvf = rt.eval_sync("[1, 2, 3];", "test_get_array.es").ok().expect("script failed");
    /// let arr: &Vec<EsValueFacade> = esvf.get_array();
    /// assert_eq!(arr.len(), 3);
    /// ```
    pub fn get_array(&self) -> &Vec<EsValueFacade> {
        self.convertible.get_array()
    }

    /// invoke the function that was returned from the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimebuilder::EsRuntimeBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    /// let rt = EsRuntimeBuilder::new().build();
    /// let func_esvf = rt.eval_sync("(function(a){return (a / 2);});", "test_invoke_function.es")
    ///     .ok().expect("script failed");
    /// // invoke the function with 18
    /// let res_esvf = func_esvf.invoke_function(vec![EsValueFacade::new_i32(18)])
    ///     .ok().expect("function failed");
    /// // check that 19 / 2 = 9
    /// let res_i32 = res_esvf.get_i32();
    /// assert_eq!(res_i32, 9);
    /// ```
    pub fn invoke_function(&self, args: Vec<EsValueFacade>) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function()");
        self.convertible.invoke_function(args)
    }

    /// check if the value is a String
    pub fn is_string(&self) -> bool {
        self.convertible.is_str()
    }

    /// check if the value is a i32
    pub fn is_i32(&self) -> bool {
        self.convertible.is_i32()
    }

    /// check if the value is a f64
    pub fn is_f64(&self) -> bool {
        self.convertible.is_f64()
    }

    /// check if the value is a bool
    pub fn is_boolean(&self) -> bool {
        self.convertible.is_bool()
    }

    /// check if the value is an object
    pub fn is_object(&self) -> bool {
        self.convertible.is_object()
    }

    /// check if the value is an array
    pub fn is_array(&self) -> bool {
        self.convertible.is_array()
    }

    /// check if the value is an function
    pub fn is_function(&self) -> bool {
        self.convertible.is_function()
    }

    pub(crate) fn to_es_value(&self, context: *mut JSContext, return_val: MutableHandleValue) {
        trace!("to_es_value.1");

        self.convertible.to_js_value(context, return_val)
    }
}

type PromiseResultContainer = Either<Result<EsValueFacade, String>, (usize, Weak<EsRuntimeInner>)>;
type PromiseResultContainerOption = Option<PromiseResultContainer>;

impl Drop for RustPromise {
    fn drop(&mut self) {
        // drop from map if val is None, task has not run yet and to_es_val was not called
        let map: &mut PromiseAnswersMap = &mut PROMISE_ANSWERS.lock("EsValueFacade::drop").unwrap();
        let id = self.id;
        if let Some(opt) = map.get(&id) {
            if opt.is_none() {
                map.remove(&id);
            }
        }
    }
}

impl Drop for CachedJSFunction {
    fn drop(&mut self) {
        let rt_arc = self.rti_ref.clone();
        let cached_obj_id = self.cached_obj_id;

        rt_arc.do_in_es_event_queue(move |_sm_rt| {
            spidermonkeyruntimewrapper::remove_cached_object(cached_obj_id);
        });
    }
}

#[cfg(test)]
mod tests {

    use crate::esruntime::EsRuntime;
    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils::EsErrorInfo;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    #[allow(clippy::float_cmp)]
    fn in_and_output_vars() {
        log::info!("test: in_and_output_vars");

        let rt: Arc<EsRuntime> = crate::esruntime::tests::TEST_RT.clone();
        rt.add_global_sync_function("test_op_0", |args: Vec<EsValueFacade>| {
            let args1 = args.get(0).expect("did not get a first arg");
            let args2 = args.get(1).expect("did not get a second arg");

            let x = args1.get_i32() as f64;
            let y = args2.get_i32() as f64;

            Ok(EsValueFacade::new_f64(x / y))
        });

        rt.add_global_sync_function("test_op_1", |args: Vec<EsValueFacade>| {
            let args1 = args.get(0).expect("did not get a first arg");
            let args2 = args.get(1).expect("did not get a second arg");

            let x = args1.get_i32();
            let y = args2.get_i32();

            Ok(EsValueFacade::new_i32(x * y))
        });

        rt.add_global_sync_function("test_op_2", |args: Vec<EsValueFacade>| {
            let args1 = args.get(0).expect("did not get a first arg");
            let args2 = args.get(1).expect("did not get a second arg");

            let x = args1.get_i32();
            let y = args2.get_i32();

            Ok(EsValueFacade::new_bool(x > y))
        });

        rt.add_global_sync_function("test_op_3", |args: Vec<EsValueFacade>| {
            let args1 = args.get(0).expect("did not get a first arg");
            let args2 = args.get(1).expect("did not get a second arg");

            let x = args1.get_i32();
            let y = args2.get_i32();

            let res_str = format!("{}", x * y);
            Ok(EsValueFacade::new_str(res_str))
        });

        let res0 = rt.eval_sync("test_op_0(13, 17);", "test_vars0.es");
        let res1 = rt.eval_sync("test_op_1(13, 17);", "test_vars1.es");
        let res2 = rt.eval_sync("test_op_2(13, 17);", "test_vars2.es");
        let res3 = rt.eval_sync("test_op_3(13, 17);", "test_vars3.es");
        let esvf0 = res0.ok().expect("1 did not get a result");
        let esvf1 = res1.ok().expect("1 did not get a result");
        let esvf2 = res2.ok().expect("2 did not get a result");
        let esvf3 = res3.ok().expect("3 did not get a result");

        assert_eq!(esvf0.get_f64(), (13_f64 / 17_f64));
        assert_eq!(esvf1.get_i32(), (13 * 17) as i32);
        assert_eq!(esvf2.get_boolean(), false);
        assert_eq!(esvf3.get_string(), format!("{}", 13 * 17).as_str());
    }

    #[test]
    fn test_wait_for_native_prom() {
        log::info!("test: test_wait_for_native_prom");

        let rt = crate::esruntime::tests::TEST_RT.clone();
        let esvf_prom = rt
            .eval_sync(
                "let p = new Promise((resolve, reject) => {resolve(123);});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;}); p;",
                "wait_for_prom.es",
            )
            .ok()
            .unwrap();
        assert!(esvf_prom.is_promise());
        let esvf_prom_resolved = esvf_prom
            .get_promise_result_blocking(Duration::from_secs(60))
            .ok()
            .unwrap()
            .ok()
            .unwrap();

        assert!(esvf_prom_resolved.is_i32());
        assert_eq!(esvf_prom_resolved.get_i32().clone(), 123 as i32);
    }

    #[test]
    fn test_wait_for_prom() {
        log::info!("test: test_wait_for_prom");

        let rt = crate::esruntime::tests::TEST_RT.clone();
        let esvf_prom = rt
            .eval_sync(
                "let test_wait_for_prom_prom = new Promise((resolve, reject) => {resolve(123);}); test_wait_for_prom_prom;",
                "wait_for_prom.es",
            )
            .ok()
            .unwrap();
        assert!(esvf_prom.is_promise());
        let esvf_prom_resolved = esvf_prom
            .get_promise_result_blocking(Duration::from_secs(60))
            .ok()
            .unwrap()
            .ok()
            .unwrap();

        assert!(esvf_prom_resolved.is_i32());
        assert_eq!(esvf_prom_resolved.get_i32().clone(), 123 as i32);
    }

    #[test]
    fn test_wait_for_prom2() {
        log::info!("test: test_wait_for_prom2");

        let rt = crate::esruntime::tests::TEST_RT.clone();

        let esvf_prom_res: Result<EsValueFacade, EsErrorInfo> = rt
            .eval_sync(
                "let test_wait_for_prom2_prom = new Promise((resolve, reject) => {console.log('rejecting promise with foo');reject(\"foo\");}); test_wait_for_prom2_prom;",
                "wait_for_prom2.es",
            );
        if esvf_prom_res.is_err() {
            panic!(
                "error evaling wait_for_prom2.es : {}",
                esvf_prom_res.err().unwrap().err_msg()
            );
        } else {
            let esvf_prom = esvf_prom_res
                .ok()
                .expect("wait_for_prom.es did not eval ok");
            assert!(esvf_prom.is_promise());
            let esvf_prom_resolved = esvf_prom
                .get_promise_result_blocking(Duration::from_secs(60))
                .ok()
                .unwrap()
                .err()
                .unwrap();

            assert!(esvf_prom_resolved.is_string());

            assert_eq!(esvf_prom_resolved.get_string(), "foo");
        }
    }

    #[test]
    fn test_wait_for_prom3() {
        log::info!("test: test_wait_for_prom3");

        let rt = crate::esruntime::tests::TEST_RT.clone();

        let my_slow_prom_esvf = EsValueFacade::new_promise(|| {
            std::thread::sleep(Duration::from_secs(10));
            Ok(EsValueFacade::new_i32(12345))
        });

        rt.eval_sync(
            "this.p3waitmethod = function(p){return p.then((res) => {return (res * 2);});};",
            "testp3.es",
        )
        .ok()
        .expect("p3 script failed");
        let prom_esvf_res = rt.call_sync(vec![], "p3waitmethod", vec![my_slow_prom_esvf]);

        if prom_esvf_res.is_err() {
            let err: EsErrorInfo = prom_esvf_res.err().unwrap();
            panic!("p3 call failed: {}", err.err_msg());
        }

        let prom_esvf = prom_esvf_res.ok().unwrap();

        let res = prom_esvf.get_promise_result_blocking(Duration::from_secs(2));
        assert!(res.is_err());
        drop(prom_esvf);
        std::thread::sleep(Duration::from_secs(10));
        // rt should still be ok here
        let _ = rt.eval_sync("true;", "p3ok.es").ok().expect("p3 not ok");
    }

    #[test]
    fn test_get_object() {
        log::info!("test: test_get_object");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let esvf = rt
            .eval_sync(
                "({a: 1, b: true, c: 'hello', d: {a: 2}});",
                "test_get_object.es",
            )
            .ok()
            .unwrap();

        assert!(esvf.is_object());

        let map: &HashMap<String, EsValueFacade> = esvf.get_object();

        let esvf_a = map.get(&"a".to_string()).unwrap();

        assert!(esvf_a.is_i32());
        assert_eq!(esvf_a.get_i32(), 1);
    }

    #[test]
    fn test_getset_array() {
        log::info!("test: test_getset_array");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let esvf = rt
            .eval_sync("([5, 7, 9]);", "test_getset_array.es")
            .ok()
            .unwrap();

        assert!(esvf.is_array());

        let vec: &Vec<EsValueFacade> = esvf.get_array();

        assert_eq!(vec.len(), 3);

        let esvf_0 = vec.get(1).unwrap();

        assert!(esvf_0.is_i32());
        assert_eq!(esvf_0.get_i32(), 7);

        let mut props = HashMap::new();
        props.insert("a".to_string(), EsValueFacade::new_i32(12));
        let new_vec = vec![
            EsValueFacade::new_i32(8),
            EsValueFacade::new_str("a".to_string()),
            EsValueFacade::new_obj(props),
        ];
        let args = vec![EsValueFacade::new_array(new_vec)];
        let res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(vec!["JSON"], "stringify", args);

        if res.is_err() {
            panic!("could not call stringify: {}", res.err().unwrap().err_msg());
        }

        let res_esvf = res.ok().unwrap();
        let str = res_esvf.get_string();
        assert_eq!(str, &"[8,\"a\",{\"a\":12}]".to_string())
    }

    #[test]
    fn test_set_object() {
        log::info!("test: test_set_object");
        let rt = crate::esruntime::tests::TEST_RT.clone();
        let _esvf = rt
            .eval_sync(
                "this.test_set_object = function test_set_object(obj, prop){return obj[prop];};",
                "test_set_object_1.es",
            )
            .ok()
            .unwrap();

        let mut map: HashMap<String, EsValueFacade> = HashMap::new();
        map.insert(
            "p1".to_string(),
            EsValueFacade::new_str("hello".to_string()),
        );
        let obj = EsValueFacade::new_obj(map);

        let res_esvf_res = rt.call_sync(
            vec![],
            "test_set_object",
            vec![obj, EsValueFacade::new_str("p1".to_string())],
        );

        let res_esvf = res_esvf_res.ok().unwrap();
        assert!(res_esvf.is_string());
        assert_eq!(res_esvf.get_string(), "hello");
    }

    #[test]
    fn test_prepped_prom() {
        log::info!("test: test_prepped_prom");
        let rt: &EsRuntime = &*crate::esruntime::tests::TEST_RT.clone();

        let my_prep_func = || {
            std::thread::sleep(Duration::from_secs(5));
            Ok(EsValueFacade::new_i32(123))
        };

        let my_bad_prep_func = || {
            std::thread::sleep(Duration::from_secs(5));
            Err("456".to_string())
        };

        let prom_esvf = EsValueFacade::new_promise(my_prep_func);
        let prom_esvf_rej = EsValueFacade::new_promise(my_bad_prep_func);

        rt.eval_sync("this.test_prepped_prom_func = (prom) => {return prom.then((p_res) => {return p_res + 'foo';}).catch((p_err) => {return p_err + 'bar';});};", "test_prepped_prom.es").ok().unwrap();

        let p2_esvf = rt.call_sync(vec![], "test_prepped_prom_func", vec![prom_esvf]);
        let p2_esvf_rej = rt.call_sync(vec![], "test_prepped_prom_func", vec![prom_esvf_rej]);

        let res = p2_esvf
            .ok()
            .unwrap()
            .get_promise_result_blocking(Duration::from_secs(10))
            .ok()
            .unwrap();

        let res_str_esvf = res.ok().unwrap();

        let res_str = res_str_esvf.get_string();

        assert_eq!("123foo", res_str);

        let res2 = p2_esvf_rej
            .ok()
            .unwrap()
            .get_promise_result_blocking(Duration::from_secs(30))
            .ok()
            .unwrap();

        let res_str_esvf_rej = res2.ok().unwrap(); // yes its the ok because we catch the rejection in test_prepped_prom.es, val should be bar thou

        let res_str_rej = res_str_esvf_rej.get_string();

        assert_eq!("456bar", res_str_rej);
    }

    #[test]
    fn test_prepped_prom_resolve() {
        log::info!("test: test_prepped_prom_resolve");
        let rt: &EsRuntime = &*crate::esruntime::tests::TEST_RT.clone();

        let my_prep_func = || {
            std::thread::sleep(Duration::from_secs(5));
            Ok(EsValueFacade::new_i32(123))
        };

        let prom_esvf = EsValueFacade::new_promise(my_prep_func);

        rt.eval_sync("this.test_prepped_prom_func = (prom) => {return prom.then((p_res) => {return p_res + 'foo';}).catch((p_err) => {return p_err + 'bar';});};", "test_prepped_prom.es").ok().unwrap();

        let p2_esvf = rt.call_sync(vec![], "test_prepped_prom_func", vec![prom_esvf]);

        let res = p2_esvf
            .ok()
            .unwrap()
            .get_promise_result_blocking(Duration::from_secs(30))
            .ok()
            .unwrap();

        let res_str_esvf = res.ok().unwrap();

        let res_str = res_str_esvf.get_string();

        assert_eq!("123foo", res_str);
    }
}
