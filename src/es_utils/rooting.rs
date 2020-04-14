use log::trace;
use mozjs::jsapi::Heap;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::{AddRawValueRoot, RemoveRawValueRoot};
use mozjs::jsval::{JSVal, ObjectValue};
use mozjs::rust::Runtime;
use std::ffi::CString;

pub fn new_persistent_rooted(context: *mut JSContext, obj: *mut JSObject) -> EsPersistentRooted {
    let mut ret = EsPersistentRooted::new();
    unsafe { ret.init(context, obj) };
    ret
}

pub struct EsPersistentRooted {
    /// The underlying `JSObject`.
    heap_obj: Heap<*mut JSObject>,
    permanent_js_root: Heap<JSVal>,
}

impl Default for EsPersistentRooted {
    fn default() -> EsPersistentRooted {
        EsPersistentRooted::new()
    }
}

impl EsPersistentRooted {
    pub fn new() -> EsPersistentRooted {
        EsPersistentRooted {
            heap_obj: Heap::default(),
            permanent_js_root: Heap::default(),
        }
    }

    pub fn new_from_obj(cx: *mut JSContext, obj: *mut JSObject) -> Self {
        let mut ret = Self::new();
        unsafe { ret.init(cx, obj) };
        ret
    }

    pub fn get(&self) -> *mut JSObject {
        self.heap_obj.get()
    }

    #[allow(unsafe_code)]
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
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe {
            // todo which thread does this? do we need a Weak<EsRuntimeInner> here to het the rt and run this drop code in the sm_rt thread (async)?
            let cx = Runtime::get();
            trace!("EsPersistentRooted -> RemoveRawValueRoot");
            RemoveRawValueRoot(cx, self.permanent_js_root.get_unsafe());
        }
    }
}

impl PartialEq for EsPersistentRooted {
    fn eq(&self, other: &EsPersistentRooted) -> bool {
        self.heap_obj.get() == other.heap_obj.get()
    }
}
