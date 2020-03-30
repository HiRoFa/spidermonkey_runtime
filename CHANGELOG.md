#0.0.3
* added utils for Promises (check if an object is a Promise and create a new Promise)
* moved function invocation util to functions.rs
* eval no longer needs a return statement
* reworking a lot of methods to use Handle instead of Value (force the invoker to root the values before doing anything)
#0.0.2
* added ability to call function in a nested object 
#0.0.1
* initial release