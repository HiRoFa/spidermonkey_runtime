use log::trace;

use crate::es_utils;
use crate::es_utils::arrays::{get_array_element, get_array_length, new_array};
use crate::es_utils::EsErrorInfo;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::{BooleanValue, DoubleValue, Int32Value, JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::{HandleObject, HandleValue, Runtime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// the EsValueFacade is a converter between rust variables and script objects
/// when receiving a EsValueFacade from the script engine it's data is always a clone from the actual data so we need not worry about the value being garbage collected
///

struct RustManagedEsVar {
    obj_id: i32,
    opt_receiver: Option<Receiver<Result<EsValueFacade, EsValueFacade>>>,
}

pub struct EsValueFacade {
    val_string: Option<String>,
    val_i32: Option<i32>,
    val_f64: Option<f64>,
    val_boolean: Option<bool>,
    val_managed_var: Option<RustManagedEsVar>,

    val_object: Option<HashMap<String, EsValueFacade>>,
    val_array: Option<Vec<EsValueFacade>>,
}

thread_local! {
    static PROMISE_RESOLUTION_TRANSMITTERS: RefCell<HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>>> =
        { RefCell::new(HashMap::new()) };
}

impl EsValueFacade {
    pub(crate) fn resolve_future(man_obj_id: i32, res: Result<EsValueFacade, EsValueFacade>) -> () {
        PROMISE_RESOLUTION_TRANSMITTERS.with(|rc| {
            let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                &mut *rc.borrow_mut();
            let opt: Option<Sender<Result<EsValueFacade, EsValueFacade>>> = map.remove(&man_obj_id);
            if opt.is_some() {
                opt.unwrap().send(res).expect("could not send res");
            } else {
                panic!("no transmitter found {}", man_obj_id);
            }
        })
    }

    pub fn undefined() -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: None,
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: None,
        }
    }

    pub fn new_f64(num: f64) -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: Some(num),
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: None,
        }
    }

    pub fn new_obj(props: HashMap<String, EsValueFacade>) -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: None,
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: Some(props),
            val_array: None,
        }
    }

    pub fn new_i32(num: i32) -> Self {
        EsValueFacade {
            val_string: None,
            val_i32: Some(num),
            val_f64: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: None,
        }
    }

    pub fn new_str(s: String) -> Self {
        EsValueFacade {
            val_string: Some(s),
            val_i32: None,
            val_f64: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: None,
        }
    }

    pub fn new_bool(b: bool) -> Self {
        EsValueFacade {
            val_string: None,
            val_i32: None,
            val_f64: None,
            val_boolean: Some(b),
            val_managed_var: None,
            val_object: None,
            val_array: None,
        }
    }

    pub fn new_array(vals: Vec<EsValueFacade>) -> Self {
        EsValueFacade {
            val_string: None,
            val_i32: None,
            val_f64: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
            val_array: Some(vals),
        }
    }

    pub fn new_v(
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

        let rval: JSVal = *rval_handle;

        if rval.is_boolean() {
            val_boolean = Some(rval.to_boolean());
        } else if rval.is_int32() {
            val_i32 = Some(rval.to_int32());
        } else if rval.is_double() {
            val_f64 = Some(rval.to_number());
        } else if rval.is_string() {
            let es_str = es_utils::es_value_to_str(context, &rval);

            trace!("EsValueFacade::new got string {}", es_str);

            val_string = Some(es_str);
        } else if rval.is_object() {
            let mut map = HashMap::new();
            let obj: *mut JSObject = rval.to_object();
            rooted!(in(context) let obj_root = obj);

            if es_utils::arrays::object_is_array(context, obj_root.handle()) {
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
                            get_res.err().unwrap().message
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
            } else if es_utils::promises::object_is_promise(context, obj_root.handle()) {
                // call esses.registerPromiseForResolutionInRust(prom);

                rooted!(in (context) let mut id_val = UndefinedValue());

                // ok it's a promise, now we're gonna call a method which will add then and catch to
                // the promise so the result is reported to rust under an id
                let reg_res: Result<(), EsErrorInfo> = es_utils::functions::call_obj_method_name(
                    context,
                    global,
                    vec!["esses"],
                    "registerPromiseForResolutionInRust",
                    vec![rval],
                    &mut id_val.handle_mut(),
                );

                if reg_res.is_err() {
                    panic!(
                        "could not reg promise due to error {}",
                        reg_res.err().unwrap().message
                    );
                } else {
                    let obj_id = id_val.to_int32();

                    let (tx, rx) = channel();
                    let opt_receiver = Some(rx);

                    PROMISE_RESOLUTION_TRANSMITTERS.with(move |rc| {
                        let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                            &mut *rc.borrow_mut();
                        map.insert(obj_id.clone(), tx);
                    });

                    let rmev: RustManagedEsVar = RustManagedEsVar {
                        obj_id: obj_id.clone(),
                        opt_receiver,
                    };

                    val_managed_var = Some(rmev);
                }
            } else {
                let prop_names: Vec<String> =
                    crate::es_utils::objects::get_js_obj_prop_names(context, obj_root.handle());
                for prop_name in prop_names {
                    rooted!(in (context) let mut prop_val_root = UndefinedValue());
                    let prop_val_res = crate::es_utils::objects::get_es_obj_prop_val(
                        context,
                        obj_root.handle(),
                        prop_name.as_str(),
                        prop_val_root.handle_mut(),
                    );

                    if prop_val_res.is_err() {
                        panic!(
                            "error getting prop {}: {}",
                            prop_name,
                            prop_val_res.err().unwrap().message
                        );
                    }

                    let prop_esvf =
                        EsValueFacade::new_v(rt, context, global, prop_val_root.handle());
                    map.insert(prop_name, prop_esvf);
                }
            }

            val_object = Some(map);
        }

        let ret = EsValueFacade {
            val_string,
            val_i32,
            val_f64,
            val_boolean,
            val_managed_var,
            val_object,
            val_array,
        };

        ret
    }

    pub fn get_string(&self) -> &String {
        self.val_string.as_ref().expect("not a string")
    }
    pub fn get_i32(&self) -> &i32 {
        &self.val_i32.as_ref().expect("i am not a i32")
    }
    pub fn get_f64(&self) -> &f64 {
        &self.val_f64.as_ref().expect("i am not a f64")
    }
    pub fn get_boolean(&self) -> bool {
        self.val_boolean.expect("i am not a boolean")
    }
    pub fn get_managed_object_id(&self) -> i32 {
        let rmev: &RustManagedEsVar = self.val_managed_var.as_ref().expect("not a managed var");
        rmev.obj_id.clone()
    }

    pub fn is_promise(&self) -> bool {
        self.is_managed_object()
    }

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

        let rx_result = rx.recv_timeout(timeout);

        return rx_result;
    }

    pub fn get_object(&self) -> &HashMap<String, EsValueFacade> {
        return self.val_object.as_ref().unwrap();
    }

    pub fn get_array(&self) -> &Vec<EsValueFacade> {
        return self.val_array.as_ref().unwrap();
    }

    pub fn is_string(&self) -> bool {
        self.val_string.is_some()
    }
    pub fn is_i32(&self) -> bool {
        self.val_i32.is_some()
    }
    pub fn is_f64(&self) -> bool {
        self.val_f64.is_some()
    }
    pub fn is_boolean(&self) -> bool {
        self.val_boolean.is_some()
    }
    pub fn is_managed_object(&self) -> bool {
        self.val_managed_var.is_some()
    }
    pub fn is_object(&self) -> bool {
        self.val_object.is_some()
    }
    pub fn is_array(&self) -> bool {
        self.val_array.is_some()
    }

    pub fn as_js_expression_str(&self) -> String {
        if self.is_boolean() {
            if self.get_boolean() {
                return "true".to_string();
            } else {
                return "false".to_string();
            }
        } else if self.is_i32() {
            return format!("{}", self.get_i32());
        } else if self.is_f64() {
            return format!("{}", self.get_f64());
        } else if self.is_string() {
            return format!("\"{}\"", self.get_string());
        } else if self.is_managed_object() {
            return format!("/* Future {} */", self.get_managed_object_id());
        } else if self.is_array() {
            // todo
            return format!("[]");
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
            return res;
        }
        "null".to_string()
    }

    pub(crate) fn to_es_value(&self, context: *mut JSContext) -> mozjs::jsapi::Value {
        trace!("to_es_value.1");

        if self.is_i32() {
            trace!("to_es_value.2");
            return Int32Value(self.get_i32().clone());
        } else if self.is_f64() {
            trace!("to_es_value.3");
            return DoubleValue(self.get_f64().clone());
        } else if self.is_boolean() {
            trace!("to_es_value.4");
            return BooleanValue(self.get_boolean());
        } else if self.is_string() {
            trace!("to_es_value.5");
            return es_utils::new_es_value_from_str(context, self.get_string());
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
            let obj: *mut JSObject = es_utils::objects::new_object(context);
            rooted!(in(context) let mut obj_root = obj);
            let map = self.get_object();
            for prop in map {
                let prop_name = prop.0;
                let prop_esvf = prop.1;
                let prop_val: mozjs::jsapi::Value = prop_esvf.to_es_value(context);
                rooted!(in(context) let mut val_root = prop_val);
                es_utils::objects::set_es_obj_prop_val(
                    context,
                    obj_root.handle(),
                    prop_name,
                    val_root.handle(),
                );
            }

            return ObjectValue(obj);
        } else {
            // todo, other val types
            trace!("to_es_value.7");
            return UndefinedValue();
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::es_utils::EsErrorInfo;
    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use log::trace;
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn in_and_output_vars() {
        println!("in_and_output_vars_1");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        rt.do_with_inner(|inner| {
            inner.register_op(
                "test_op_0",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32().clone() as f64;
                    let y = args2.get_i32().clone() as f64;

                    return Ok(EsValueFacade::new_f64(x / y));
                }),
            );
            inner.register_op(
                "test_op_1",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    return Ok(EsValueFacade::new_i32(x * y));
                }),
            );

            inner.register_op(
                "test_op_2",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    return Ok(EsValueFacade::new_bool(x > y));
                }),
            );

            inner.register_op(
                "test_op_3",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    let res_str = format!("{}", x * y);
                    return Ok(EsValueFacade::new_str(res_str));
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

            assert_eq!(esvf0.get_f64().clone(), (13 as f64 / 17 as f64));
            assert_eq!(esvf1.get_i32().clone(), (13 * 17) as i32);
            assert_eq!(esvf2.get_boolean(), false);
            assert_eq!(esvf3.get_string(), format!("{}", 13 * 17).as_str());
        });
    }

    #[test]
    fn test_wait_for_native_prom() {
        println!("test_wait_for_native_prom");

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
        println!("test_wait_for_prom_1");

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
        trace!("test_wait_for_prom_2");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();

        let esvf_prom_res: Result<EsValueFacade, EsErrorInfo> = rt
            .eval_sync(
                "let test_wait_for_prom2_prom = new Promise((resolve, reject) => {console.log('rejecting promise with foo');reject(\"foo\");}); test_wait_for_prom2_prom;",
                "wait_for_prom2.es",
            );
        if esvf_prom_res.is_err() {
            panic!(
                "error evaling wait_for_prom2.es : {}",
                esvf_prom_res.err().unwrap().message
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
            panic!(res.err().unwrap().message);
        }

        let res_esvf = res.ok().unwrap();
        let str = res_esvf.get_string();
        assert_eq!(str, &"[8,\"a\",{\"a\":12}]".to_string())
    }

    #[test]
    fn test_set_object() {
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
}
