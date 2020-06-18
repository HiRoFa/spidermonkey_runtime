use log::trace;
use mozjs::jsapi::Heap;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::{AddRawValueRoot, RemoveRawValueRoot};
use mozjs::jsval::{JSVal, ObjectValue};

use mozjs::rust::Runtime;
use std::ffi::CString;

/// the EsPersistentRooted struct is used to keep an Object rooted while there are no references to it in the script Runtime
/// the root will be released when this struct is dropped
pub struct EsPersistentRooted {
    /// The underlying `JSObject`.
    heap_obj: Box<Heap<*mut JSObject>>,
    permanent_js_root: Box<Heap<JSVal>>,
}

impl Default for EsPersistentRooted {
    fn default() -> EsPersistentRooted {
        EsPersistentRooted::new()
    }
}

impl EsPersistentRooted {
    pub fn new() -> EsPersistentRooted {
        EsPersistentRooted {
            heap_obj: Box::new(Heap::default()),
            permanent_js_root: Box::new(Heap::default()),
        }
    }

    /// create a new instance of EsPersistentRooted with a given JSObject
    /// this will init the EsPersistentRooted and thus the obejct wille be rooted after calling this method
    pub fn new_from_obj(cx: *mut JSContext, obj: *mut JSObject) -> Self {
        let mut ret = Self::new();
        unsafe { ret.init(cx, obj) };
        ret
    }

    /// get the JSObject rooted by this instance of EsPersistentRooted
    pub fn get(&self) -> *mut JSObject {
        self.heap_obj.get()
    }

    /// # Safety
    /// be safe :)
    pub unsafe fn init(&mut self, cx: *mut JSContext, js_obj: *mut JSObject) {
        self.heap_obj.set(js_obj);
        self.permanent_js_root.set(ObjectValue(js_obj));
        let c_str = CString::new("EsPersistentRooted::root").unwrap();
        trace!("EsPersistentRooted -> AddRawValueRoot");
        assert!(AddRawValueRoot(
            cx,
            self.permanent_js_root.get_unsafe(),
            c_str.as_ptr() as *const i8
        ));
    }
}

impl Drop for EsPersistentRooted {
    fn drop(&mut self) {
        unsafe {
            let cx = Runtime::get();
            trace!("EsPersistentRooted -> RemoveRawValueRoot");
            RemoveRawValueRoot(cx, self.permanent_js_root.get_unsafe());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::jsapi_utils::rooting::EsPersistentRooted;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use mozjs::jsval::Int32Value;

    #[test]
    fn test_rooting1() {
        log::info!("test_rooting1 ");

        let rt = crate::esruntime::tests::TEST_RT.clone();

        let res = rt.do_with_inner(|inner| {
            inner.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntime| {
                let mut vec = vec![];
                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                    //crate::jsapi_utils::set_gc_zeal_options(cx);
                    crate::jsapi_utils::gc(cx);
                    let my_obj = crate::jsapi_utils::objects::new_object(cx);
                    let mut r = EsPersistentRooted::new();
                    unsafe { r.init(cx, my_obj) };
                    // let r = EsRootedObject::new(cx, my_obj);
                    // move r to vec;
                    vec.push(r);
                });
                sm_rt.do_with_jsapi(|_rt, cx, _global| {
                    rooted!(in (cx) let my_obj_pval = Int32Value(123));
                    rooted!(in (cx) let my_obj_root = vec.get(0).unwrap().get());
                    crate::jsapi_utils::objects::set_es_obj_prop_val(
                        cx,
                        my_obj_root.handle(),
                        "p1",
                        my_obj_pval.handle(),
                    );
                });
                for _x in 0..100 {
                    sm_rt.do_with_jsapi(|_rt, cx, _global| {
                        crate::jsapi_utils::gc(cx);
                        rooted!(in (cx) let my_obj_root = vec.get(0).unwrap().get());
                        // my_obj should be quite dead here if rooting is borked
                        {
                            let i = crate::jsapi_utils::objects::get_es_obj_prop_val_as_i32(
                                cx,
                                my_obj_root.handle(),
                                "p1",
                            );
                            assert_eq!(i, 123);
                        }
                    });
                }
                true
            }))
        });

        assert_eq!(true, res);
    }
}
