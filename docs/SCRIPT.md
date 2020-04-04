# Script API

in the script engine there a global object called esses. this object contains the util functions needed to perform 
actions for rust-ecmascript interoperability.

## doing stuff async

There are two ways to run async code from javascript:

* use setImmediate()

```ecmascript
let myAsyncMethod = function(a, b){
    console.log("multiplying %i and %i", a, b);
    return a * b;
};
setImmediate(myAsyncMethod, 12, 14);
```

* create a Promise
```ecmascript
new Promise((resolve, reject) => {
    try {
        resolve(myAsyncMethod(2, 3));
    } catch(ex) {
        reject(ex);
    }
}).then((answer) => {
    console.info("axb=%i", answer);
});
```

## calling rust ops

There are 3 methods in the esses object to call a rust op.

the preferred way is one of the asynchronous version.

Currently they just run async in the same single threaded ThreadPool but 
in the future they will run async a a multithreaded pool.

### esses.invoke_rust_op(op_name, ...args);

Returns a Promise which will resolve with the resulting value of the rust op.

```ecmascript
esses.invoke_rust_op("my_rusty_op", 3, 6).then((answer) => {
    console.info("your rusty answer was %s", answer);
});
```

### esses.invoke_rust_op_sync(op_name, ...args);

Returns the resulting value of the rust op synchronously.

```ecmascript
let answer = esses.invoke_rust_op_sync("my_rusty_op", 3, 6);
console.info("your rusty answer was %s", answer);
```

### esses.invoke_rust_op_void(op_name, ...args);

Will run the rust op asynchronously but never return a value to the script engine.

```ecmascript
esses.invoke_rust_op_void("my_rusty_op", 3, 6);
console.info("no one knows when your rust op will run");
```

## logging to console

in the console.rs a console object is added to the global scope.

The log, debug, info, trace, error and warn methods are implemented with the option to pass a string and substitutions.

```ecmascript
console.info("my name is %s and i <s>am</s> feel %i years old", "Andries", 25);
```

### Using string substitutions

When passing a string to one of the console object's methods that accepts a string (such as log()), you may use these substitution strings:

| Substitution string | Description |
| ------ | ------ |
| %d or %i | Outputs an integer. Number formatting is supported, for example ```console.log("Foo %.2d", 1.1)``` will output the number as two significant figures with a leading 0: ```Foo 01``` |
| %s | Outputs a string. |
| %f | Outputs a floating-point value. Formatting is supported, for example ```console.log("Foo %.2f", 1.1)``` will output the number to 2 decimal places: ```Foo 1.10``` |
