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

## passing variables from and to script

Variables are passed using a EsValueFacade object, this object copies values from and to the Runtime so you need not worry about garbage collection.

Creating a new EsValueFacade can be done by calling one of the EsValueFacade::new_* methods

Getting a rust var from an EsValueFacade is done by suing the EsValueFacade.to_* methods

For example

```rust
fn test_esvf(rt: &EsRuntimeWrapper){
    // create a map top represent an es object like {a:12}
    let mut props = HashMap::new();
    // every sub prop is als an EsValueFacade
    props.insert("a".to_string(), EsValueFacade::new_i32(12));
    // create a vec to represent an array like [8,"a",{a:12}]
    let new_vec = vec![
        EsValueFacade::new_i32(8),
        EsValueFacade::new_str("a".to_string()),
        EsValueFacade::new_obj(props),
    ];
    // create an EsValueFacade for the vec
    let args = vec![EsValueFacade::new_array(new_vec)];
  
    // call JSON.stringify with out new array as param
    let res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(vec!["JSON"], "stringify", args);

    // check error
    if res.is_err() {
        panic!(res.err().unwrap().message);
    }
 
    // get result string
    let res_esvf = res.ok().unwrap();
    let str = res_esvf.get_string();
    assert_eq!(str, &"[8,\"a\",{\"a\":12}]".to_string())
}
```

## Adding features

// todo