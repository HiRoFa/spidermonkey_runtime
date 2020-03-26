# es_runtime

es_runtime is a crate aimed at making it possible for rust devs to integrate an ECMA-Script engine in their rust projects

The engine used is mozilla's SpiderMonkey engine (https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey)

This project was started as a hobby project for me to learn rust. I hope some of you find it usefull to learn about using spidermonkey from rust.

# status

Nowhere near production ready, it is untested and i'm pretty sure i created some memory leaks in the unsafe sections...

It works with the mozjs crate version 0.10.1 which is allready pretty old but there are no newer releases, when I get more comfortable with spidermonkey and mozjs I'll see about using a git pull of a newer version.

Currently i'm working towards creating a 0.1 version which has a couple of goals

* [x] easy loading script files
* [x] error handling (get ES errors in rust with filename/linenumber etc)
* [x] adding rust function to the engine so they are callable from ECMA-Script
  * [x] blocking
  * [x] non-blocking (returns a Promise in script)
* [x] easy way to call ECMA-Script functions from rust
  * [x] by name (run_global_function())
  * [ ] by objectname and name (myObj.doSomething())
  * [x] passing params from rust
* [ ] getting data from engine as primitives or vecs and maps
  * [x] primitives
  * [x] objects from and to maps
  * [ ] arrays as vecs
* [x] working console (logging)
* [x] working Promises in Script
* [x] waiting for Promises from rust
* [ ] no more memory leaks

# future goals / todo's

* [ ] typedArrays from and to vecs
* [ ] typescript support
* [ ] import/export statement support
* [ ] much more

# Other plans

I'm also working on a more feature rich runtime with a commandline tool and also an application server based on this runtime

These are in a very early testing stage and may become available later as a seperate project.

# examples

Cargo.toml

```toml
    [dependencies]
    es_runtime = "0.0.1"
```

my_app.rs

```rust

    #[test]
    fn example() {
        // start a runtime

        let rt = EsRuntimeWrapper::new(None);

        // create an example object

        rt.eval_sync("this.myObj = {a: 1, b: 2};", "test1.es")
            .ok()
            .unwrap();

        // register a native rust method

        rt.register_op(
            "my_rusty_op",
            Box::new(|_sm_rt, args: Vec<EsValueFacade>| {
                let a = args.get(0).unwrap().get_i32();
                let b = args.get(1).unwrap().get_i32();
                Ok(EsValueFacade::new_i32(a * b))
            }),
        );

        // call the rust method from ES

        rt.eval_sync(
            "this.myObj.c = esses.invoke_rust_op_sync('my_rusty_op', 3, 7);",
            "test2.es",
        )
        .ok()
        .unwrap();

        let c: Result<EsValueFacade, EsErrorInfo> =
            rt.eval_sync("return(this.myObj.c);", "test3.es");

        assert_eq!(&21, c.ok().unwrap().get_i32());

        // define an ES method and calling it from rust

        rt.eval_sync("this.my_method = (a, b) => {return a * b;};", "test4.es")
            .ok()
            .unwrap();

        let args = vec![EsValueFacade::new_i32(12), EsValueFacade::new_i32(5)];
        let c_res: Result<EsValueFacade, EsErrorInfo> = rt.call_sync("my_method", args);
        let c: EsValueFacade = c_res.ok().unwrap();
        assert_eq!(&60, c.get_i32());
    }



```

# a word on compiling

Currently I have only compiled this on and for a 64 bit linux machine (I use openSUSE) 

Besides rust (duh) you'll need to install the following packages to compile the mozjs crate

* gcc-7
* autoconf2.13
* automake
* clang
* python
