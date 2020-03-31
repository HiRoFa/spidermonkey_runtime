#0.0.4
* added utils for arrays and objects (arrays.rs and obejcts.rs)
* refactoring of getting obj props (root first)
* created eval_void methods to eval without copying result to EsValueFacade
* EsValueFacade can now be used to convert Vecs to Arrays and vice versa
#0.0.3
* added utils for Promises (check if an object is a Promise and create a new Promise)
* moved function invocation util to functions.rs
* eval no longer needs a return statement
* reworking a lot of methods to use Handle instead of Value (force the invoker to root the values before doing anything)
#0.0.2
* added ability to call function in a nested object 
#0.0.1
* initial release
