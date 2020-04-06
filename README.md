# es_runtime

es_runtime is a crate aimed at making it possible for rust developers to integrate an ECMA-Script engine in their rust projects without having specialized knowledge about ECMA-Script engines.

The engine used is the Mozilla SpiderMonkey engine (https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey).

This project was started as a hobby project for me to learn rust. I hope some of you find it useful to learn about using spidermonkey from rust.

# status

Nowhere near production ready, it is untested...

It works with the mozjs crate version 0.10.1 which is already pretty old but there are no newer releases, when i get more comfortable with spidermonkey and mozjs i'll see about using a git pull of a newer version.

Please see the [CHANGELOG](CHANGELOG.md) for what's new.

0.1 works with mozjs 0.10.1 and meets the goals i've set

For 0.2 the goals is mostly the same but with a much newer mozjs 

# 0.1 goals

* [x] Get a grip on when to use rooted values (Handle) and when to use Values (JSVal) 
* [x] Easy loading script files
* [x] Error handling (get ES errors in rust with filename/linenumber etc)
* [x] Adding rust function to the engine so they are callable from ECMA-Script
  * [x] Blocking
  * [x] Non-blocking (returns a Promise in script)
* [x] Easy way to call ECMA-Script functions from rust
  * [x] By name (run_global_function())
  * [x] By object name and name (myObj.doSomething())
  * [x] Passing params from rust
* [x] Getting data from engine as primitives or Vecs and Maps
  * [x] Primitives
  * [x] Objects from and to Maps
  * [x] Arrays as Vecs
* [x] Working console (logging)
* [x] Working Promises in Script
* [x] Waiting for Promises from rust
* [x] import/export statement support
  * [x] cache modules
* [x] No more memory leaks

# 0.2 goals

* [ ] use newer mozjs

# 0.2.1 goals

* [ ] run rust-ops multithreaded
* [ ] typedArrays from and to Vecs
* [ ] complete set of from/to primitives in EsValueFacade

# 0.2.2 goals

* [ ] Use PersistentRooted instead of deprecated Add\*Root and Remove\*Root

# 1.0 goals

* [ ] No more segfaults in unit test with gc_zeal_options

# 2.0 goals

* [ ] TypeScript support
* [ ] Interactive Debugging
* [ ] Profiling
* [ ] much more

# Other plans

I'm also working on a more feature rich runtime with a commandline tool and also an application server based on this runtime

These are in a very early testing stage and may become available later as a separate project.

I'dd like to hear what you would want to see in this project and or what you'd like to use it for, please drop me a line @ incoming+drfos-es-runtime-17727229-issue-@incoming.gitlab.com

# examples

Cargo.toml

```toml
[dependencies]
es_runtime = "0.1"
```

my_app.rs

```rust

    #[test]
    fn example() {
        // start a runtime

        let rt = EsRuntimeWrapper::builder()
                    // run the garbage collector every 5 secs
                    .gc_interval(Duration::from_secs(5))
                    .build();
    
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
            rt.eval_sync("(this.myObj.c);", "test3.es");

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

Besides rust you'll need to install the following packages to compile the mozjs crate

* gcc-7
* autoconf2.13
* automake
* clang
* python

for more detailed info please visit https://github.com/servo/mozjs#building 

# howtos

[HOWTO](docs/HOWTO.md)