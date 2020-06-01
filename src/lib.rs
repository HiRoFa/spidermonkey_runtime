#[macro_use]
extern crate mozjs;

extern crate libc;
extern crate log;

#[macro_use]
extern crate lazy_static;

mod debugmutex;
mod enginehandleproducer;
mod es_sys_scripts;
#[macro_use]
pub mod es_utils;
//pub mod esreflection; // i'm leaving this out for now
pub mod esruntimewrapper;
pub mod esruntimewrapperbuilder;
pub mod esruntimewrapperinner;
pub mod esvaluefacade;
mod features;
mod microtaskmanager;
pub mod spidermonkeyruntimewrapper;
mod taskmanager;

/// # es_runtime
/// There are basicly two ways to use this crate, the easy way and the JSAPI way
///
/// If you just want to quickly add a script engine to your project your best bet is the easy way
///
/// you can add methods to the engine by [add_rust_op] and you deal with values through the EsValueFacade struct
///
/// If you need or want to use the JSAPI methods you can use call the EsRuntimeWrapper::do_in_es_runtime_thread(_sync) method to add a job to the TaskManager which will run all tasks for the script engine
///
/// # Examples
///
/// ```rust
//
// fn call_jsapi_stuff(rt: &EsRuntimeWrapper) {
//     let res = rt.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntimeWrapper| {
//
//         // do_with_jsapi does a couple of things
//         // 1.  root the global obj
//         // 2.   enter the correct Comparment using AutoCompartment
//         sm_rt.do_with_jsapi(|runtime: &Runtime, context: *mut JSContext, global_handle: HandleObject| {
//
//             // work with JSAPI methods here
//             // there are utils in es_utils.rs with examples of working with JSAPI objects and functions.
//
//             // return a value
//             true
//         })
//
//     });
// }
//
// ```
