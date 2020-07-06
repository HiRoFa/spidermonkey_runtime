use mozjs::jsapi::Handle as RawHandle;
use mozjs::jsapi::MutableHandle as RawMutableHandle;
use mozjs::rust::{Handle, MutableHandle};

/// convert a mozjs::rust::Handle<T> to a mozjs::jsapi::Handle<T>
pub fn raw_handle<T>(handle: Handle<T>) -> RawHandle<T> {
    handle.into()
}

/// convert a mozjs::rust::Handle<T> to a mozjs::jsapi::Handle<T>
pub fn raw_handle_mut<T>(handle: MutableHandle<T>) -> RawMutableHandle<T> {
    handle.into()
}

/// convert a mozjs::jsapi::Handle<T> to a mozjs::rust::Handle<T>
pub fn from_raw_handle<'a, T>(raw_handle: RawHandle<T>) -> Handle<'a, T> {
    unsafe { Handle::from_raw(raw_handle) }
}

/// convert a mozjs::jsapi::Handle<T> to a mozjs::rust::Handle<T>
pub fn from_raw_handle_mut<'a, T>(raw_handle: RawMutableHandle<T>) -> MutableHandle<'a, T> {
    unsafe { MutableHandle::from_raw(raw_handle) }
}
