# howto

in the script engine there a global object called esses. this object contains the util functions needed to perform 
actions for rust-ecmascript interoperability.

## logging to console

in the console.rs a console object is added to the global scope.

I've implemented the log, debug, info, trace, error and warn methods with the option to pass a string and substitutions.

```ecmascript
console.info("my name is %s and i <s>am</s> feel %i years old", "Andries", 25);
```

## doing stuff async

There are two ways to run async code from javascript

* use setImmediate()

```ecmascript
let myAsyncMethod = function(a, b){
    console.log("multiplying %i and %i", a, b);
    return a * b;
};
setImmediate(myAsyncMethod, a, b);
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

## loading a script file

To add a script file to your rust project you can load it in the runtime by calling eval_sync.

```rust
fn load_file(rt: &EsRuntimeWrapper) {
    let my_script_code = include_str!("myscripts/script1.es");
    let init_res = rt.eval_sync(my_script_code, "script1.es");
    if !init_res.is_ok() {
        let esei = init_res.err().unwrap();
        panic!(
            "could not init file: {} at {}:{}:{} ",
            esei.message, esei.filename, esei.lineno, esei.column
        );
    }
}
```

## calling a script function from rust

calling a script function from rust is done by using the call or call_sync methods

```ecmascript
this.myObj = {childObj: {myMethod: function(a, b){return a*b;}}};
```

```rust
fn call_method(rt: &EsRuntimeWrapper) {
    let call_res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(vec!["myObj", "childObj"], "myMethod", vec![EsValueFacade::new_i32(12), EsValueFacade::new_i32(14)]);
    match call_res {
        Ok(esvf) => println!("answer was {}", esvf.get_i32()),
        Err(eei) => println!("failed because {}", eei.message)
    }
}
```
 