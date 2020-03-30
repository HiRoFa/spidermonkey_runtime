# Rust API

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

## Adding features

// todo