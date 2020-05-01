# JSAPI API

In order to work with JSAPI you need to run a rust closure in the worker thread of the SmRuntimeWrapper.

the easiest way to do that is by calling EsRuntimeWrapper.do_in_es_runtime_thread_sync

```rust

fn call_jsapi_stuff(rt: &EsRuntimeWrapper) {
    let res = rt.do_in_es_runtime_thread_sync(|sm_rt: &SmRuntimeWrapper| {
    
        // do_with_jsapi does a couple of things
        // 1.  root the global obj
        // 2.   enter the correct Comparment using AutoCompartment        
        sm_rt.do_with_jsapi(|runtime: &Runtime, context: *mut JSContext, global_handle: HandleObject| {

            // work with JSAPI methods here
            // there are utils in es_utils.rs with examples of working with JSAPI objects and functions.
    
            // return a value
            true
        })

    });
}

```

## about JSAPI

// todo
* function ref
* mozjs docs
* howto rooting

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

## proxies

U can use the Proxy struct in es_utils::reflection to create a proxy object

```rust

let proxy_arc = es_utils::reflection::ProxyBuilder::new("TestClass1")
    // if you want to create a proxy that can be constructed you need to pass a constructor
    // you need to generate an id here for your object
    // if you don't pass a constructor you can only use the static_* methods to create events, properties and methods
    .constructor(|cx: *mut JSContext, args: &CallArgs| {
        // this will run in the sm_rt workerthread so global is rooted here
        debug!("proxytest: construct");
        Ok(1)
    })
    // create a property and pass a closure for the get and set action
    .property("foo", |obj_id| {
        debug!("proxy.get foo {}", obj_id);
        Int32Value(123)
    }, |obj_id, _val| {
        debug!("proxy.set foo {}", obj_id);
    })
    // the finalizer is called when the instance is garbage collected, use this to drop your own object in rust
    .finalizer(|id: &i32| {
        debug!("proxytest: finalize id {}", id);
    })
    // a method for your instance
    .method("methodA", |obj_id, args| {
        trace!("proxy.methodA called for obj {} with {} args", obj_id, args.argc_);
        UndefinedValue()
    })
    // and an event that may be dispatched
    .event("saved")
    // when done build your proxy
    .build(cx, global);

    let esvf = sm_rt.eval(
        "// create a new instance of your Proxy\n\
             let tp_obj = new TestClass1('bar'); \n\
         // you can set props that are not proxied \n\
             tp_obj.abc = 1; console.log('tp_obj.abc = %s', tp_obj.abc); \n\
         // test you getter and setter\n\
             let i = tp_obj.foo; tp_obj.foo = 987; \n\
         // test your method\n\
             tp_obj.methodA(1, 2, 3); \n\
         // add an event listener\n\
             tp_obj.addEventListener('saved', (evt) => {console.log('tp_obj was saved');}); \n\
         // dispatch an event from script\n\
             tp_obj.dispatchEvent('saved', {}); \n\
         // allow you object to be GCed\n\
             tp_obj = null; i;",
        "test_proxy.es",
    ).ok().unwrap();

    assert_eq!(&123, esvf.get_i32());
 
   // dispatch event from rust
   proxy.dispatch_event(1, "saved", cx, UndefinedValue());

``` 
