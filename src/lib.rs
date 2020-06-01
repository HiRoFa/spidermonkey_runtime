//!
//! Welcome to the es_runtime crate
//!
//! es_runtime is aimed at making it possible for rust developers to integrate an ECMA-Script engine in their rust projects without having specialized knowledge about ECMA-Script engines.
//!
//! The engine used is the Mozilla SpiderMonkey engine (https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey).
//!
//! There are basicly two ways of using this lib
//!
//! If you don't feel like using the JSAPI, which comes with quite a manual then you can use the utils in this project which use the EsValueFacade struct to pass variables around.
//!
//! There is also number of utils which allow you to use the JSAPI, these can be accessed by calling EsRuntimeWrapper.do_in_es_runtime_thread(_sync)
//!
//! # Example
//!
//! ```rust
//!
//! use mozjs::rust::{Runtime, HandleObject};
//! use mozjs::jsapi::JSContext;
//! use es_runtime::spidermonkeyruntimewrapper::SmRuntime;
//! fn call_jsapi_stuff() {
//!     let rt = es_runtime::esruntimewrapper::EsRuntimeWrapper::builder().build();
//!     let res = rt.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntime| {
//!     
//!         // do_with_jsapi does a couple of things
//!         // 1.  root the global obj
//!         // 2.   enter the correct Comparment using AutoCompartment        
//!         sm_rt.do_with_jsapi(|runtime: &Runtime, context: *mut JSContext, global_handle: HandleObject| {
//!
//!             // work with JSAPI methods here
//!             // there are utils in es_utils.rs with examples of working with JSAPI objects and functions.
//!     
//!             // return a value
//!             true
//!         })
//!
//!     });
//! }
//!
//! ```
//!

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
