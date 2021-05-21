//!
//! # Welcome to the spidermonkey_runtime crate
//!
//! spidermonkey_runtime is aimed at making it possible for rust developers to integrate a script engine in their rust projects without having specialized knowledge about that script engines.
//!
//! The engine used is Mozilla SpiderMonkey (https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey).
//!
//! There are basicly two ways of using this lib
//!
//! ## Using EsValueFacade
//!
//! If you don't feel like using the JSAPI, which comes with quite a learning curve. then you can use the utils in this project which use the EsValueFacade struct to pass variables around.
//!
//! ### Examples
//!
//! Using EsValueFacade:
//!
//! ```no_run
//!
//! use spidermonkey_runtime::esvaluefacade::EsValueFacade;
//! use spidermonkey_runtime::jsapi_utils::EsErrorInfo;
//! fn use_es_value_facade() {
//!     let rt = spidermonkey_runtime::esruntime::EsRuntime::builder().build();
//!     rt.eval_sync("let my_public_method = function(a, b){console.log(\"my_public_method called with: a=%s b=%s\", a, b);};", "my_script.es");
//!     let a = EsValueFacade::new_str(format!("abc"));
//!     let b = EsValueFacade::new_str(format!("def"));
//!     let res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(vec![], "my_public_method", vec![a, b]);
//!     assert!(res.is_ok());
//! }
//!
//! ```
//!
//! you can also define a function in rust that may be called from script
//!
//! ```no_run
//!
//! use spidermonkey_runtime::esvaluefacade::EsValueFacade;
//! fn define_function(){
//!     let rt = spidermonkey_runtime::esruntime::EsRuntime::builder().build();
//!     // using the async variant means the function will return as a Promise
//!     rt.add_global_async_function("my_function", |args: Vec<EsValueFacade>| {
//!          println!("rust closure was called from script");
//!          Ok(EsValueFacade::undefined())
//!     });
//!     rt.eval_sync("my_function();", "define_function.es").ok().unwrap();
//! }
//!
//! ```
//!
//!
//! ## Using JSAPI
//!
//! There is also number of utils which allow you to use the JSAPI, these can be accessed by calling EsRuntimeWrapper.do_in_spidermonkey_runtime_thread(_sync).
//!
//! utils can be found in the spidermonkey_runtime::jsapi_utils package.
//!
//! ### Examples
//!
//! Using JSAPI:
//!
//! ```no_run
//!
//! use mozjs::rust::{Runtime, HandleObject};
//! use mozjs::jsapi::JSContext;
//! use spidermonkey_runtime::spidermonkeyruntimewrapper::SmRuntime;
//!
//! fn use_jsapi() {
//!     let rt = spidermonkey_runtime::esruntime::EsRuntime::builder().build();
//!     // first of all we need to run a closure in the event queue for the engine
//!     let res = rt.do_in_es_event_queue_sync(|sm_rt: &SmRuntime| {
//!         // then we tell the SmRuntime we want to use the JSAPI
//!         // do_with_jsapi does a couple of things
//!         // 1.  root the global obj
//!         // 2.   enter the correct Compartment using AutoCompartment        
//!         sm_rt.do_with_jsapi(|runtime: &Runtime, context: *mut JSContext, global_handle: HandleObject| {
//!
//!             // work with JSAPI methods here
//!             // there are utils in jsapi_utils.rs with examples of working with JSAPI objects and functions.
//!     
//!             // return a value
//!             true
//!         });
//!
//!         // you can also define a global function which can be called from script
//!         sm_rt.add_global_function("my_function", |_cx, _callargs| {
//!              println!("rust function was called from script");
//!              true
//!         });
//!
//!         true
//!
//!     });
//! }
//! ```
//!

#[macro_use]
extern crate mozjs;

extern crate libc;
extern crate log;

#[macro_use]
extern crate lazy_static;

mod es_sys_scripts;
#[macro_use]

pub mod esreflection;
pub mod esruntime;
pub mod esruntimebuilder;
pub mod esruntimeinner;
pub mod esvaluefacade;
mod features;
pub mod jsapi_utils;
pub mod spidermonkeyruntimewrapper;
