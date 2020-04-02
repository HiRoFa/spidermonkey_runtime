# JSAPI API

In order to work with JSAPI you need to run a rust closure in the worker thread of the SmRuntimeWrapper.

the easiest way to do that is by calling EsRuntimeWrapper.do_in_es_runtime_thread_sync

```rust

fn call_jsapi_stuff(rt: &EsRuntimeWrapper) {
    let res = rt.do_in_es_runtime_thread_sync(Box::new(|sm_rt: &SmRuntimeWrapper| {
        
        let runtime: &mozjs::rust::Runtime = &sm_rt.runtime;
        let context: *mut JSContext = runtime.cx();

        // always root the global obj before doing anything
        rooted!(in(context) let global_root = sm_rt.global_obj);

        // work with JSAPI methods here
        // there are utils in es_utils.rs with examples of working with JSAPI objects and functions.
    
        // return a value
        true

    }));
}

```

## objects

utils in es_utils::objects

### Creating a new Object

// todo

### Getting and settings properties of an Object

// todo

## functions

utils in es_utils::functions

// todo

### Creating a new Function

// todo

## arrays

utils in es_utils::arrays

// todo

### Creating a new Array

// todo

### Getting array size and adding values

// todo

## promises

utils in es_utils::promises

// todo

## modules

utils in es_utils::modules

// todo

