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
    let call_res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync(
        vec!["myObj", "childObj"], 
        "myMethod", 
        vec![EsValueFacade::new_i32(12), EsValueFacade::new_i32(14)]
        );
    match call_res {
        Ok(esvf) => println!("answer was {}", esvf.get_i32()),
        Err(eei) => println!("failed because {}", eei.message)
    }
}
```

## passing variables from and to script

Variables are passed using a EsValueFacade object, this object copies values from
and to the Runtime so you need not worry about garbage collection.

Creating a new EsValueFacade can be done by calling one of the EsValueFacade::new_* methods

Getting a rust var from an EsValueFacade is done by suing the EsValueFacade.to_* methods

For example:

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

### Waiting for a Promise to resolve

When a script returns a Promise you can wait for the Promise to resolve by 
calling **get_promise_result_blocking** on the EsValueFacade

```rust
let code = "\
let async_method = async function(){\
    let p = new Promise((resolve, reject) => {\
        setImmediate(() => {\
            resolve(123);\
        });\
    });\
return p;\
};\
 \
let async_method_2 = async function(){\
    let res = await async_method();\
    return res;\
}; \
async_method_2();\
";
        
let prom_facade = rt.eval_sync(code, "call_method").ok().unwrap();
let wait_res = prom_facade.get_promise_result_blocking(Duration::from_secs(5));
let prom_res = wait_res.ok().unwrap();
let esvf_res = prom_res.ok().unwrap();
assert_eq!(&123, esvf_res.get_i32());
```

## Adding features

// todo