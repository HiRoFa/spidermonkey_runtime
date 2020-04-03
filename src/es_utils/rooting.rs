use mozjs::jsapi::Heap;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::{AddRawValueRoot, RemoveRawValueRoot};
use mozjs::jsval::{JSVal, ObjectValue};
use mozjs::rust::Runtime;
use std::ffi::CString;

pub fn new_persistent_rooted(context: *mut JSContext, obj: *mut JSObject) -> MyPersistentRooted {
    let mut ret = MyPersistentRooted::new();
    unsafe { ret.init(context, obj) };
    ret
}

pub struct MyPersistentRooted {
    /// The underlying `JSObject`.
    heap_obj: Heap<*mut JSObject>,
    permanent_js_root: Heap<JSVal>,
}

impl Default for MyPersistentRooted {
    fn default() -> MyPersistentRooted {
        MyPersistentRooted::new()
    }
}

impl MyPersistentRooted {
    fn new() -> MyPersistentRooted {
        MyPersistentRooted {
            heap_obj: Heap::default(),
            permanent_js_root: Heap::default(),
        }
    }

    pub fn get(&self) -> *mut JSObject {
        self.heap_obj.get()
    }

    #[allow(unsafe_code)]
    unsafe fn init(&mut self, cx: *mut JSContext, js_obj: *mut JSObject) {
        self.heap_obj.set(js_obj);
        self.permanent_js_root.set(ObjectValue(js_obj));
        let c_str = CString::new("MyPersistentRooted::root").unwrap();
        assert!(AddRawValueRoot(
            cx,
            self.permanent_js_root.get_unsafe(),
            c_str.as_ptr() as *const i8
        ));
    }
}

impl Drop for MyPersistentRooted {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe {
            let cx = Runtime::get();
            RemoveRawValueRoot(cx, self.permanent_js_root.get_unsafe());
        }
    }
}
