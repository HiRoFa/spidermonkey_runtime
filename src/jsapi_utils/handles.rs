use mozjs::jsapi::Handle as RawHandle;
use mozjs::jsapi::MutableHandle as RawMutableHandle;
use mozjs::rust::{Handle, MutableHandle};

/// convert a mozjs::rust::Handle<T> to a mozjs::jsapi::Handle<T>
pub fn raw_handle<T>(handle: Handle<T>) -> RawHandle<T> {
    unsafe { RawHandle::from_marked_location(&*handle) }
}

/// convert a mozjs::rust::Handle<T> to a mozjs::jsapi::Handle<T>
pub fn raw_handle_mut<T>(mut handle: MutableHandle<T>) -> RawMutableHandle<T> {
    unsafe { RawMutableHandle::from_marked_location(&mut *handle) }
}

/// convert a mozjs::jsapi::Handle<T> to a mozjs::rust::Handle<T>
pub fn from_raw_handle<'a, T>(raw_handle: RawHandle<T>) -> Handle<'a, T> {
    unsafe { Handle::from_marked_location(&*raw_handle) }
}

/// convert a mozjs::jsapi::Handle<T> to a mozjs::rust::Handle<T>
pub fn from_raw_handle_mut<'a, T>(mut raw_handle: RawMutableHandle<T>) -> MutableHandle<'a, T> {
    unsafe { MutableHandle::from_marked_location(&mut *raw_handle) }
}
