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
