use log::trace;

use crate::debugmutex::DebugMutex;
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use crate::jsapi_utils;
use crate::jsapi_utils::arrays::{get_array_element, get_array_length, new_array};
use crate::jsapi_utils::rooting::EsPersistentRooted;
use crate::jsapi_utils::EsErrorInfo;
use crate::spidermonkeyruntimewrapper::SmRuntime;
use crate::utils::AutoIdMap;
use either::Either;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::{BooleanValue, DoubleValue, Int32Value, JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::{HandleObject, HandleValue, Runtime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Weak};
use std::time::Duration;

struct RustManagedEsVar {
    obj_id: i32,
    opt_receiver: Option<Receiver<Result<EsValueFacade, EsValueFacade>>>,
}

/// the EsValueFacade is a converter between rust variables and script objects
/// when receiving a EsValueFacade from the script engine it's data is always a clone from the actual data so we need not worry about the value being garbage collected
///
/// # Example
///
/// ```no_run
/// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
///
/// let rt = EsRuntimeWrapperBuilder::default().build();
/// let esvf = rt.eval_sync("123", "test_es_value_facade.es").ok().unwrap();
/// assert!(esvf.is_i32());
/// assert_eq!(esvf.get_i32(), &123);
/// ```
pub struct EsValueFacade {
    val_string: Option<String>,
    val_i32: Option<i32>,
    val_f64: Option<f64>,
    val_boolean: Option<bool>,
    val_managed_var: Option<RustManagedEsVar>,
    val_object: Option<HashMap<String, EsValueFacade>>,
    val_array: Option<Vec<EsValueFacade>>,
    val_promise: Option<usize>,
    val_js_function: Option<(usize, Arc<EsRuntimeWrapperInner>)>,
}

thread_local! {
    static PROMISE_RESOLUTION_TRANSMITTERS: RefCell<HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>>> = RefCell::new(HashMap::new());
}

type PromiseAnswersMap = AutoIdMap<PromiseResultContainerOption>;

lazy_static! {
    static ref PROMISE_ANSWERS: Arc<DebugMutex<PromiseAnswersMap>> =
        Arc::new(DebugMutex::new(AutoIdMap::new(), "PROMISE_ANSWERS"));
}

impl EsValueFacade {
    pub(crate) fn resolve_future(man_obj_id: i32, res: Result<EsValueFacade, EsValueFacade>) {
        PROMISE_RESOLUTION_TRANSMITTERS.with(|rc| {
            let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                &mut *rc.borrow_mut();
            let opt: Option<Sender<Result<EsValueFacade, EsValueFacade>>> = map.remove(&man_obj_id);
            if let Some(opt_c) = opt {
                opt_c.send(res).expect("could not send res");
            } else {
                panic!("no transmitter found {}", man_obj_id);
            }
        })
    }

    /// create a new EsValueFacade representing an undefined value
    pub fn undefined() -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: None,
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: None,
            val_promise: None,
            val_js_function: None,
        }
    }

    /// create a new EsValueFacade representing a float
    pub fn new_f64(num: f64) -> Self {
        let mut ret = Self::undefined();
        ret.val_f64 = Some(num);
        ret
    }

    /// create a new EsValueFacade representing a basic object with properties as defined in the HashMap
    pub fn new_obj(props: HashMap<String, EsValueFacade>) -> Self {
        let mut ret = Self::undefined();
        ret.val_object = Some(props);
        ret
    }

    /// create a new EsValueFacade representing a signed integer
    pub fn new_i32(num: i32) -> Self {
        let mut ret = Self::undefined();
        ret.val_i32 = Some(num);
        ret
    }

    /// create a new EsValueFacade representing a String
    pub fn new_str(s: String) -> Self {
        let mut ret = Self::undefined();
        ret.val_string = Some(s);
        ret
    }

    /// create a new EsValueFacade representing a bool
    pub fn new_bool(b: bool) -> Self {
        let mut ret = Self::undefined();
        ret.val_boolean = Some(b);
        ret
    }

    /// create a new EsValueFacade representing an Array
    pub fn new_array(vals: Vec<EsValueFacade>) -> Self {
        let mut ret = Self::undefined();
        ret.val_array = Some(vals);
        ret
    }

    /// create a new EsValueFacade representing a Promise, the passed closure will actually run in a seperate helper thread and resolve the Promise that is created in the script runtime
    ///
    /// # Example
    ///
    /// ```no_run
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    /// use std::time::Duration;
    ///
    /// let rt = EsRuntimeWrapperBuilder::new().build();
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
                        rti.do_in_es_runtime_thread_sync(Box::new(move |sm_rt: &SmRuntime| {
                            // resolve or reject promise
                            sm_rt.do_with_jsapi(move|_rt, cx, _global| {

                                let prom_obj: *mut JSObject = {
                                    let prom_epr: EsPersistentRooted = crate::spidermonkeyruntimewrapper::consume_cached_object(prom_regged_id);
                                    trace!("epr should drop here");
                                    prom_epr.get()
                                };
                                trace!("epr should be dropped here");
                                rooted!(in (cx) let mut prom_obj_root = prom_obj);
                                trace!("rooted promise");

                                if res.is_ok() {
                                    trace!("rooting result");
                                    rooted!(in (cx) let res_root = res.ok().unwrap().to_es_value(cx));
                                    trace!("resolving prom");
                                    let resolve_prom_res = jsapi_utils::promises::resolve_promise(
                                        cx,
                                        prom_obj_root.handle(),
                                        res_root.handle(),
                                    );
                                    if resolve_prom_res.is_err() {
                                        panic!("could not resolve promise {} because of error: {}", prom_regged_id, resolve_prom_res.err().unwrap().err_msg());
                                    }
                                } else {
                                    trace!("rooting err result");
                                    let err_str = res.err().unwrap();
                                    let err_val = jsapi_utils::new_es_value_from_str(cx, err_str.as_str());
                                    rooted!(in (cx) let res_root = err_val);
                                    trace!("rejecting prom");
                                    let reject_prom_res = jsapi_utils::promises::reject_promise(
                                        cx,
                                        prom_obj_root.handle(),
                                        res_root.handle(),
                                    );
                                    if reject_prom_res.is_err() {
                                        panic!("could not reject promise {} because of error: {}", prom_regged_id, reject_prom_res.err().unwrap().err_msg());
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
        crate::esruntimewrapper::EsRuntimeWrapper::add_helper_task(task);

        let mut ret = Self::undefined();

        ret.val_promise = Some(id);

        ret
    }

    pub(crate) fn new_v(
        rt: &Runtime,
        context: *mut JSContext,
        global: HandleObject,
        rval_handle: HandleValue,
    ) -> Self {
        let mut val_string = None;
        let mut val_i32 = None;
        let mut val_f64 = None;
        let mut val_boolean = None;
        let mut val_managed_var = None;
        let mut val_object = None;
        let mut val_array = None;
        let mut val_js_function = None;

        let rval: JSVal = *rval_handle;

        if rval.is_boolean() {
            val_boolean = Some(rval.to_boolean());
        } else if rval.is_int32() {
            val_i32 = Some(rval.to_int32());
        } else if rval.is_double() {
            val_f64 = Some(rval.to_number());
        } else if rval.is_string() {
            let es_str = jsapi_utils::es_value_to_str(context, rval).ok().unwrap();

            trace!("EsValueFacade::new got string {}", es_str);

            val_string = Some(es_str);
        } else if rval.is_object() {
            let mut map = HashMap::new();
            let obj: *mut JSObject = rval.to_object();
            rooted!(in(context) let obj_root = obj);

            if jsapi_utils::arrays::object_is_array(context, obj_root.handle()) {
                let mut vals = vec![];
                // add vals

                let arr_len = get_array_length(context, obj_root.handle()).ok().unwrap();
                for x in 0..arr_len - 1 {
                    rooted!(in (context) let mut arr_element_root = UndefinedValue());
                    let get_res = get_array_element(
                        context,
                        obj_root.handle(),
                        x,
                        arr_element_root.handle_mut(),
                    );
                    if get_res.is_err() {
                        panic!(
                            "could not get element of array: {}",
                            get_res.err().unwrap().err_msg()
                        );
                    }
                    vals.push(EsValueFacade::new_v(
                        rt,
                        context,
                        global,
                        arr_element_root.handle(),
                    ));
                }

                val_array = Some(vals);
            } else if jsapi_utils::promises::object_is_promise(context, obj_root.handle()) {
                // call esses.registerPromiseForResolutionInRust(prom);

                rooted!(in (context) let mut id_val = UndefinedValue());

                // ok it's a promise, now we're gonna call a method which will add then and catch to
                // the promise so the result is reported to rust under an id
                let reg_res: Result<(), EsErrorInfo> = jsapi_utils::functions::call_obj_method_name(
                    context,
                    global,
                    vec!["esses"],
                    "registerPromiseForResolutionInRust",
                    vec![rval],
                    id_val.handle_mut(),
                );

                if reg_res.is_err() {
                    panic!(
                        "could not reg promise due to error {}",
                        reg_res.err().unwrap().err_msg()
                    );
                } else {
                    let obj_id = id_val.to_int32();

                    let (tx, rx) = channel();
                    let opt_receiver = Some(rx);

                    PROMISE_RESOLUTION_TRANSMITTERS.with(move |rc| {
                        let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                            &mut *rc.borrow_mut();
                        map.insert(obj_id, tx);
                    });

                    let rmev: RustManagedEsVar = RustManagedEsVar {
                        obj_id,
                        opt_receiver,
                    };

                    val_managed_var = Some(rmev);
                }
            } else if jsapi_utils::functions::object_is_function(obj) {
                // wrap function in persistentrooted

                let rti_ref = crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                    let sm_rt: &SmRuntime = &*sm_rt_rc.borrow();
                    sm_rt.clone_rtw_inner()
                });
                let cached_id =
                    crate::spidermonkeyruntimewrapper::register_cached_object(context, obj);
                val_js_function = Some((cached_id, rti_ref));
            } else {
                let prop_names: Vec<String> =
                    crate::jsapi_utils::objects::get_js_obj_prop_names(context, obj_root.handle());
                for prop_name in prop_names {
                    rooted!(in (context) let mut prop_val_root = UndefinedValue());
                    let prop_val_res = crate::jsapi_utils::objects::get_es_obj_prop_val(
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

                    let prop_esvf =
                        EsValueFacade::new_v(rt, context, global, prop_val_root.handle());
                    map.insert(prop_name, prop_esvf);
                }
            }

            val_object = Some(map);
        }

        EsValueFacade {
            val_string,
            val_i32,
            val_f64,
            val_boolean,
            val_managed_var,
            val_object,
            val_array,
            val_promise: None,
            val_js_function,
        }
    }

    /// get the String value
    pub fn get_string(&self) -> &String {
        self.val_string.as_ref().expect("not a string")
    }

    /// get the i32 value
    pub fn get_i32(&self) -> &i32 {
        &self.val_i32.as_ref().expect("i am not a i32")
    }

    /// get the f64 value
    pub fn get_f64(&self) -> &f64 {
        &self.val_f64.as_ref().expect("i am not a f64")
    }

    /// get the boolean value
    pub fn get_boolean(&self) -> bool {
        self.val_boolean.expect("i am not a boolean")
    }

    pub fn get_managed_object_id(&self) -> i32 {
        let rmev: &RustManagedEsVar = self.val_managed_var.as_ref().expect("not a managed var");
        rmev.obj_id
    }

    /// check if this esvf was a promise which was returned from the script engine
    pub fn is_promise(&self) -> bool {
        self.is_managed_object()
    }

    /// check if this esvf was a promise which was initialized from rust by calling EsValueFacade::new_promise()
    pub fn is_prepped_promise(&self) -> bool {
        self.val_promise.is_some()
    }

    /// wait for a promise to resolve in rust
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use std::time::Duration;
    ///
    /// let rt = EsRuntimeWrapperBuilder::new().build();
    /// // run the script and fail if script fails
    /// let esvf_prom = rt.eval_sync("let p = new Promise((resolve, reject) => {setImmediate(() => {resolve(123);});}); p;", "test_get_promise_result_blocking.es").ok().expect("script failed");
    /// // wait for the promise or fail on timeout
    /// let wait_res = esvf_prom.get_promise_result_blocking(Duration::from_secs(1)).ok().expect("promise timed out");
    /// // get the ok result, fail is promise was rejected
    /// let esvf = wait_res.ok().expect("promise was rejected");
    /// // check the result
    /// assert_eq!(esvf.get_i32(), &123);
    /// ```
    pub fn get_promise_result_blocking(
        &self,
        timeout: Duration,
    ) -> Result<Result<EsValueFacade, EsValueFacade>, RecvTimeoutError> {
        if !self.is_promise() {
            return Ok(Err(EsValueFacade::new_str(
                "esvf was not a Promise".to_string(),
            )));
        }

        let rmev: &RustManagedEsVar = self.val_managed_var.as_ref().expect("not a managed var");
        let rx = rmev.opt_receiver.as_ref().expect("not a waiting promise");
        rx.recv_timeout(timeout)
    }

    /// get the value as a Map of EsValueFacades, this works when the value was an object in the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    ///
    /// let rt = EsRuntimeWrapperBuilder::new().build();
    /// let esvf = rt.eval_sync("{a: 1, b: 2};", "test_get_object.es").ok().expect("script failed");
    /// let map = esvf.get_object();
    /// assert!(map.contains_key("a"));
    /// assert!(map.contains_key("b"));
    /// ```
    pub fn get_object(&self) -> &HashMap<String, EsValueFacade> {
        self.val_object.as_ref().unwrap()
    }

    /// get the value as a Vec of EsValueFacades, this works when the value was an array in the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    /// let rt = EsRuntimeWrapperBuilder::new().build();
    /// let esvf = rt.eval_sync("[1, 2, 3];", "test_get_array.es").ok().expect("script failed");
    /// let arr: &Vec<EsValueFacade> = esvf.get_array();
    /// assert_eq!(arr.len(), 3);
    /// ```
    pub fn get_array(&self) -> &Vec<EsValueFacade> {
        self.val_array.as_ref().unwrap()
    }

    /// invoke the function that was returned from the script engine
    /// # Example
    /// ```no_run
    /// use es_runtime::esruntimewrapperbuilder::EsRuntimeWrapperBuilder;
    /// use es_runtime::esvaluefacade::EsValueFacade;
    ///
    /// let rt = EsRuntimeWrapperBuilder::new().build();
    /// let func_esvf = rt.eval_sync("(function(a){return (a / 2);});", "test_invoke_function.es").ok().expect("script failed");
    /// // invoke the function with 18
    /// let res_esvf = func_esvf.invoke_function(vec![EsValueFacade::new_i32(18)]).ok().expect("function failed");
    /// // check that 19 / 2 = 9
    /// let res_i32 = res_esvf.get_i32();
    /// assert_eq!(res_i32, &9);
    /// ```
    pub fn invoke_function(&self, args: Vec<EsValueFacade>) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function()");
        let rt_arc = self.val_js_function.as_ref().unwrap().1.clone();
        let cached_id = self.val_js_function.as_ref().unwrap().0;

        let job = move |sm_rt: &SmRuntime| Self::invoke_function2(cached_id, sm_rt, args);

        rt_arc.do_in_es_runtime_thread_sync(job)
    }

    pub(crate) fn invoke_function2(
        cached_id: usize,
        sm_rt: &SmRuntime,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function2()");
        sm_rt
            .do_with_jsapi(|rt, cx, global| Self::invoke_function3(cached_id, rt, cx, global, args))
    }

    pub(crate) fn invoke_function3(
        cached_id: usize,
        rt: &Runtime,
        cx: *mut JSContext,
        global: HandleObject,
        args: Vec<EsValueFacade>,
    ) -> Result<EsValueFacade, EsErrorInfo> {
        trace!("EsValueFacade.invoke_function3()");
        crate::spidermonkeyruntimewrapper::do_with_cached_object(
            cached_id,
            |epr: &EsPersistentRooted| {
                let mut arguments_value_vec: Vec<JSVal> = vec![];
                for arg_vf in &args {
                    // todo root these
                    arguments_value_vec.push(arg_vf.to_es_value(cx));
                }

                rooted!(in (cx) let mut rval = UndefinedValue());
                rooted!(in (cx) let scope = mozjs::jsval::NullValue().to_object_or_null());
                rooted!(in (cx) let function_val = mozjs::jsval::ObjectValue(epr.get()));

                let res2: Result<(), EsErrorInfo> = jsapi_utils::functions::call_method_value(
                    cx,
                    scope.handle(),
                    function_val.handle(),
                    arguments_value_vec,
                    rval.handle_mut(),
                );

                if res2.is_ok() {
                    Ok(EsValueFacade::new_v(rt, cx, global, rval.handle()))
                } else {
                    Err(res2.err().unwrap())
                }
            },
        )
    }

    /// check if the value is a String
    pub fn is_string(&self) -> bool {
        self.val_string.is_some()
    }

    /// check if the value is a i32
    pub fn is_i32(&self) -> bool {
        self.val_i32.is_some()
    }

    /// check if the value is a f64
    pub fn is_f64(&self) -> bool {
        self.val_f64.is_some()
    }

    /// check if the value is a bool
    pub fn is_boolean(&self) -> bool {
        self.val_boolean.is_some()
    }

    /// check if the value is a promise
    pub fn is_managed_object(&self) -> bool {
        self.val_managed_var.is_some()
    }

    /// check if the value is an object
    pub fn is_object(&self) -> bool {
        self.val_object.is_some()
    }

    /// check if the value is an array
    pub fn is_array(&self) -> bool {
        self.val_array.is_some()
    }

    /// check if the value is an function
    pub fn is_function(&self) -> bool {
        self.val_js_function.is_some()
    }

    pub fn as_js_expression_str(&self) -> String {
        if self.is_boolean() {
            if self.get_boolean() {
                "true".to_string()
            } else {
                "false".to_string()
            }
        } else if self.is_i32() {
            format!("{}", self.get_i32())
        } else if self.is_f64() {
            format!("{}", self.get_f64())
        } else if self.is_string() {
            format!("\"{}\"", self.get_string())
        } else if self.is_managed_object() {
            format!("/* Future {} */", self.get_managed_object_id())
        } else if self.is_array() {
            // todo
            "[]".to_string()
        } else if self.is_object() {
            let mut res: String = String::new();
            let map = self.get_object();
            res.push('{');
            for e in map {
                if res.len() > 1 {
                    res.push_str(", ");
                }
                res.push('"');
                res.push_str(e.0);
                res.push_str("\": ");

                res.push_str(e.1.as_js_expression_str().as_str());
            }

            res.push('}');
            res
        } else {
            "null".to_string()
        }
    }

    // todo, refactor to have a rval: MutableHandleValue
    pub(crate) fn to_es_value(&self, context: *mut JSContext) -> mozjs::jsapi::Value {
        trace!("to_es_value.1");

        if self.is_i32() {
            trace!("to_es_value.2");
            Int32Value(*self.get_i32())
        } else if self.is_f64() {
            trace!("to_es_value.3");
            DoubleValue(*self.get_f64())
        } else if self.is_boolean() {
            trace!("to_es_value.4");
            BooleanValue(self.get_boolean())
        } else if self.is_string() {
            trace!("to_es_value.5");
            jsapi_utils::new_es_value_from_str(context, self.get_string())
        } else if self.is_array() {
            let mut items = vec![];
            for item in self.val_array.as_ref().unwrap() {
                items.push(item.to_es_value(context));
            }

            rooted!(in (context) let mut arr_root = UndefinedValue());

            new_array(context, items, &mut arr_root.handle_mut());
            let val: JSVal = *arr_root;
            val
        } else if self.is_object() {
            trace!("to_es_value.6");
            let obj: *mut JSObject = jsapi_utils::objects::new_object(context);
            rooted!(in(context) let mut obj_root = obj);
            let map = self.get_object();
            for prop in map {
                let prop_name = prop.0;
                let prop_esvf = prop.1;
                let prop_val: mozjs::jsapi::Value = prop_esvf.to_es_value(context);
                rooted!(in(context) let mut val_root = prop_val);
                jsapi_utils::objects::set_es_obj_prop_val(
                    context,
                    obj_root.handle(),
                    prop_name,
                    val_root.handle(),
                );
            }

            ObjectValue(obj)
        } else if self.is_prepped_promise() {
            self.to_es_promise_value(context)

        // todo
        } else {
            // todo, other val types
            trace!("to_es_value.7");
            UndefinedValue()
        }
    }

    fn to_es_promise_value(&self, context: *mut JSContext) -> JSVal {
        trace!("to_es_value.7 prepped_promise");
        let map: &mut PromiseAnswersMap = &mut PROMISE_ANSWERS.lock("to_es_value.7").unwrap();
        let id = self.val_promise.as_ref().unwrap();
        if let Some(opt) = map.get(id) {
            trace!("create promise");
            // create promise
            let prom = jsapi_utils::promises::new_promise(context);
            trace!("rooting promise");
            rooted!(in (context) let prom_root = prom);

            if opt.is_none() {
                trace!("set rooted Promise obj and weakref in right");
                // set rooted Promise obj and weakref in right

                let (pid, rti_ref) = crate::spidermonkeyruntimewrapper::SM_RT.with(|sm_rt_rc| {
                    let sm_rt: &SmRuntime = &*sm_rt_rc.borrow();

                    let pid =
                        crate::spidermonkeyruntimewrapper::register_cached_object(context, prom);

                    let weakref = sm_rt.opt_es_rt_inner.as_ref().unwrap().clone();

                    (pid, weakref)
                });
                map.replace(id, Some(Either::Right((pid, rti_ref))));
            } else {
                trace!("remove eith from map and resolve promise with left");
                // remove eith from map and resolve promise with left
                let eith = map.remove(id).unwrap();

                if eith.is_left() {
                    let res = eith.left().unwrap();
                    if res.is_ok() {
                        rooted!(in (context) let res_root = res.ok().unwrap().to_es_value(context));
                        let prom_reso_res = jsapi_utils::promises::resolve_promise(
                            context,
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
                        let err_val = jsapi_utils::new_es_value_from_str(context, err_str.as_str());
                        rooted!(in (context) let res_root = err_val);

                        let prom_reje_res = jsapi_utils::promises::reject_promise(
                            context,
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
            ObjectValue(prom)
        } else {
            panic!("PROMISE_ANSWERS had no val for id {}", id);
        }
    }
}

type PromiseResultContainer =
    Either<Result<EsValueFacade, String>, (usize, Weak<EsRuntimeWrapperInner>)>;
type PromiseResultContainerOption = Option<PromiseResultContainer>;

impl Drop for EsValueFacade {
    fn drop(&mut self) {
        if self.is_prepped_promise() {
            // drop from map if val is None, task has not run yet and to_es_val was not called
            let map: &mut PromiseAnswersMap =
                &mut PROMISE_ANSWERS.lock("EsValueFacade::drop").unwrap();
            let id = self.val_promise.as_ref().unwrap();
            if let Some(opt) = map.get(id) {
                if opt.is_none() {
                    map.remove(id);
                }
            }
        } else if self.is_function() {
            let rt_arc = self.val_js_function.as_ref().unwrap().1.clone();
            let cached_obj_id = self.val_js_function.as_ref().unwrap().0;

            rt_arc.do_in_es_runtime_thread(move |_sm_rt| {
                crate::spidermonkeyruntimewrapper::consume_cached_object(cached_obj_id);
            });
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::esruntimewrapper::EsRuntimeWrapper;
    use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
    use crate::esvaluefacade::EsValueFacade;
    use crate::jsapi_utils::EsErrorInfo;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    #[allow(clippy::float_cmp)]
    fn in_and_output_vars() {
        log::info!("test: in_and_output_vars");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        rt.do_with_inner(|inner| {
            inner.register_op(
                "test_op_0",
                Arc::new(|_rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = *args1.get_i32() as f64;
                    let y = *args2.get_i32() as f64;

                    Ok(EsValueFacade::new_f64(x / y))
                }),
            );
            inner.register_op(
                "test_op_1",
                Arc::new(|_rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    Ok(EsValueFacade::new_i32(x * y))
                }),
            );

            inner.register_op(
                "test_op_2",
                Arc::new(|_rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    Ok(EsValueFacade::new_bool(x > y))
                }),
            );

            inner.register_op(
                "test_op_3",
                Arc::new(|_rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    let res_str = format!("{}", x * y);
                    Ok(EsValueFacade::new_str(res_str))
                }),
            );

            let res0 = inner.eval_sync(
                "esses.invoke_rust_op_sync('test_op_0', 13, 17);",
                "test_vars0.es",
            );
            let res1 = inner.eval_sync(
                "esses.invoke_rust_op_sync('test_op_1', 13, 17);",
                "test_vars1.es",
            );
            let res2 = inner.eval_sync(
                "esses.invoke_rust_op_sync('test_op_2', 13, 17);",
                "test_vars2.es",
            );
            let res3 = inner.eval_sync(
                "esses.invoke_rust_op_sync('test_op_3', 13, 17);",
                "test_vars3.es",
            );
            let esvf0 = res0.ok().expect("1 did not get a result");
            let esvf1 = res1.ok().expect("1 did not get a result");
            let esvf2 = res2.ok().expect("2 did not get a result");
            let esvf3 = res3.ok().expect("3 did not get a result");

            assert_eq!(*esvf0.get_f64(), (13_f64 / 17_f64));
            assert_eq!(esvf1.get_i32().clone(), (13 * 17) as i32);
            assert_eq!(esvf2.get_boolean(), false);
            assert_eq!(esvf3.get_string(), format!("{}", 13 * 17).as_str());
        });
    }

    #[test]
    fn in_and_output_vars2() {
        log::info!("test: in_and_output_vars2");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        rt.do_with_inner(|inner: &EsRuntimeWrapperInner| {
            inner.register_op(
                "test_op_4",
                Arc::new(|_rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
                    let func = args.get(0).expect("need at least one arg");

                    assert!(func.is_function());

                    let a1 = EsValueFacade::new_i32(3);
                    let a2 = EsValueFacade::new_i32(7);

                    let res = func.invoke_function(vec![a1, a2]);

                    if res.is_ok() {
                        Ok(res.ok().unwrap())
                    } else {
                        Err(res.err().unwrap().err_msg())
                    }
                }),
            );

            let res4 = inner.eval_sync(
                "esses.invoke_rust_op_sync('test_op_4', (a, b) => {return a * b;});",
                "test_vars4.es",
            );

            let esvf4 = res4.ok().expect("4 did not get a result");

            assert_eq!(esvf4.get_i32().clone(), (7 * 3) as i32);
        });
    }

    #[test]
    fn test_wait_for_native_prom() {
        log::info!("test: test_wait_for_native_prom");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
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

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
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

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

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
    fn test_get_object() {
        log::info!("test: test_get_object");
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
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
        assert_eq!(esvf_a.get_i32(), &1);
    }

    #[test]
    fn test_getset_array() {
        log::info!("test: test_getset_array");
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let esvf = rt
            .eval_sync("([5, 7, 9]);", "test_getset_array.es")
            .ok()
            .unwrap();

        assert!(esvf.is_array());

        let vec: &Vec<EsValueFacade> = esvf.get_array();

        let esvf_0 = vec.get(1).unwrap();

        assert!(esvf_0.is_i32());
        assert_eq!(esvf_0.get_i32(), &7);

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
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
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
        let rt: &EsRuntimeWrapper = &*crate::esruntimewrapper::tests::TEST_RT.clone();

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

        assert_eq!(&"123foo", res_str);

        let res2 = p2_esvf_rej
            .ok()
            .unwrap()
            .get_promise_result_blocking(Duration::from_secs(10))
            .ok()
            .unwrap();

        let res_str_esvf_rej = res2.ok().unwrap(); // yes its the ok because we catch the rejection in test_prepped_prom.es, val should be bar thou

        let res_str_rej = res_str_esvf_rej.get_string();

        assert_eq!(&"456bar", res_str_rej);
    }

    #[test]
    fn test_prepped_prom_resolve() {
        log::info!("test: test_prepped_prom_resolve");
        let rt: &EsRuntimeWrapper = &*crate::esruntimewrapper::tests::TEST_RT.clone();

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
            .get_promise_result_blocking(Duration::from_secs(10))
            .ok()
            .unwrap();

        let res_str_esvf = res.ok().unwrap();

        let res_str = res_str_esvf.get_string();

        assert_eq!(&"123foo", res_str);
    }
}
