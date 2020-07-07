# 0.5.0 (work in progress)

* Script support
  * Dynamic module imports
  ```javascript
    let {my, module_stuf} = await import('my_module.mes');
    // or
    import('my_module.mes').then((my_module) => {
         my_module.my();
         let b = my_module.module_stuff;
    });
  ```
* internals
  * renamed MicroTaskManager to EsEventQueue
  * relative path support for module loading (#32) (the module now has a String argument which contains the path of the module which is loading the sub module)
  * deprecated and removed invoke_rust_op (replaced with add_global_(a)sync_function)
* jsapi_utils
  * added utils for converting Handle to RawHandle an vice-versa
  * added util for reporting exceptions (jsapi_utils::report_exception)
  * added utils for compiling script to a JSScript object and executing it
  * added utils for compiling script to a JSFunction object
  * renamed function util methods from \*method* to \*function*

# 0.4.0

* changed the way you add and invoke rust methods 
  * EsRuntime::add_global_(a)sync_function
  * SmRuntime::add_global_function
* added EsProxy(Builder) for reflecting rust objects while using EsValueFacade as arguments and return types
* changed the methods/getters for Proxy (rval: MutableHandleValue instead of returning a JSVal)
* lots of inline documentation added
* renamed es_utils to jsapi_utils
* renamed EsRuntimeWrapper(Builder/Inner) to EsRuntime(Builder/Inner)

# 0.3.4 / 0.3.5

* minor updates

# 0.3.3

* Support JSNative methods for proxy classes

# 0.3.2

* Proxy class (JSAPI)

# 0.3.1 

* removed necessity to box closures when calling EsRuntimeWrapper::run_in_es_runtime_thread*()
* added possibility to invoke EsValueFacade when it wraps a JS function. E.g. when calling a rust-op with a function arguments to be used as consumer
* added utils for constructing objects based on a constructor

# 0.3.0

* broke compatibility because of changed interface for rust-ops, hence version jumped to 0.3
* rust-ops now run async and return a promise from rust
* lots of threading related issues, mostly about preventing deadlocks when using EsValueFacade::new_promise()

# 0.2.2

* EsValueFacade::new_promise, create a facade with a closure which will be run async, results in a Promise being passed to the script runtime
* promise instantiation, resolution and rejection in es_utils::promises 

# 0.2.1

* init code revamp, now supports creating, dropping and recreating multiple runtimes

# 0.2.0

* use latest mozjs from github

# 0.1.0

* Builder for EsRuntimeWrapper
* console.assert

# 0.0.6

* major refactor of how to use the JSAPI, this is now always done by ```sm_rt.do_with_jsapi()``` so we can more predictably root the global and enter the compartment.
* caching of modules, so they really only run once 

# 0.0.5

* added support and utils for modules (see [RUST#modules](docs/RUST.md#loading-files-while-using-modules))
* added a constructor for runtime for use with modules
 * currently modules are not cached and thus loaded several times, will be fixed later

# 0.0.4

* added utils for arrays and objects (arrays.rs and objects.rs)
* refactoring of getting obj props (root first)
* created eval_void methods to eval without copying result to EsValueFacade
* EsValueFacade can now be used to convert Vecs to Arrays and vice versa

# 0.0.3

* added utils for Promises (check if an object is a Promise and create a new Promise)
* moved function invocation util to functions.rs
* eval no longer needs a return statement
* reworking a lot of methods to use Handle instead of Value (force the invoker to root the values before doing anything)

# 0.0.2

* added ability to call function in a nested object 

# 0.0.1

* initial release
