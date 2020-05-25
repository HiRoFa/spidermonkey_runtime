# es_runtime

es_runtime is a crate aimed at making it possible for rust developers to integrate an ECMA-Script engine in their rust projects without having specialized knowledge about ECMA-Script engines.

The engine used is the Mozilla SpiderMonkey engine (https://developer.mozilla.org/en-US/docs/Mozilla/Projects/SpiderMonkey).

This project started as a hobby project for me to learn rust. I hope some of you find it useful to learn about using spidermonkey from rust.

# Status

Nowhere near production ready, it is untested...

From 0.2.0 it works with the latest mozjs crate version 0.13.0.  
0.1.0 and older worked with the mozjs 0.10.1 release. (mozjs does not publish new release anymore because of a [bug](https://github.com/rust-lang/cargo/issues/6917) in cargo)

Please see the [CHANGELOG](CHANGELOG.md) for what's new.

# Goals

Embedding a script engine in a rust project seems a very tedious job which involves learning a lot about the inner workings of that engine.

The main goal of this project is to make that job **easy**!

The manner in which this is achieved is primarily focused on abstracting the workings of the engine from the implementor, therefore some functionality may not be the fastest way of getting things done.

So a second goal is to make implementing a fast and efficient integration doable for the uninitiated, the most common tasks you do with the engine should be doable with the utils in this package and working examples should be provided in the test modules.

The reason I chose SpiderMonkey as the engine is that I've been dealing with less modern engines in my java projects and not being able to use the latest and greatest ECMA-script features becomes quite disappointing at times.    

## 0.1 Goals

* [x] Get a grip on when to use rooted values (Handle) and when to use Values (JSVal) 
* [x] Easy loading script files
* [x] Error handling (get ES errors in rust with filename/linenumber etc)
* [x] Adding rust function to the engine, so they are callable from ECMA-Script
  * [x] Blocking
  * [x] Non-blocking (returns a Promise in script)
* [x] Easy way to call ECMA-Script functions from rust
  * [x] By name (run_global_function())
  * [x] By object name and name (myObj.doSomething())
  * [x] Passing params from rust
* [x] Getting data from the engine as primitives or Vecs and Maps
  * [x] Primitives
  * [x] Objects from and to Maps
  * [x] Arrays as Vecs
* [x] A working console (logging)
* [x] Working Promises in Script
* [x] Waiting for Promises from rust
* [x] import/export statement support
  * [x] cache modules
* [x] No more memory leaks

## 0.2 goals

* [x] Use newer mozjs

## 0.2.1 goals

* [x] Re-enable possibility to create multiple runtimes

## 0.2.2 goals

* [x] Init a EsValueFacade as a promise from rust, #19
* [x] More tests for e.g. error handling in module loading, error handling in promises

## 0.3.0 

* [x] Run rust-ops multithreaded, and return Promises from rust #8

## 0.3.x

* [x] Pass functions as consumer argument to rust-ops
* [x] Proxy class for rust objects
* [ ] Dynamic imports #20
* [ ] TypedArrays from and to Vecs
* [ ] Complete set of from/to primitives in EsValueFacade

## 0.9 goals

* [ ] Use PersistentRooted instead of deprecated Add\*Root and Remove\*Root

## 1.0 goals

* [ ] No more segfaults in unit test with gc_zeal_options

## 2.0 goals

* [ ] TypeScript support
* [ ] Interactive Debugging
* [ ] Profiling
* [ ] Much more

# Other plans

I'm also working on a more feature rich runtime with a commandline tool, and an application server based on this runtime.

These are in a very early testing stage and may become available later as a separate project.

I'dd like to hear what you would want to see in this project and or what you'd like to use it for, please drop me a line @ [my gitlab](mailto:incoming+drfos-es-runtime-17727229-issue-@incoming.gitlab.com).

# Examples

Cargo.toml

```toml
[dependencies]
# latest tag
es_runtime = {git = "https://gitlab.com/drfos/es_runtime.git", tag = "0.3.3"}
# or just get the latest
# es_runtime = {git = "https://gitlab.com/drfos/es_runtime.git"}

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
            Arc::new(|_rt, args: Vec<EsValueFacade>| {
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

# A word on compiling

Currently, I have only compiled this on and for a 64 bit linux machine (I use openSUSE).

Besides rust, you'll need to install the following packages to compile the mozjs crate.

* from 0.0.1
    * gcc-7
    * autoconf2.13
    * automake
    * clang
    * python
    * llvm (mozjs_sys needs llvm-objdump)
* on openSUSE i also need
  * python-xml
  * gcc-c

for more detailed info please visit https://github.com/servo/mozjs#building 

# Howtos

[HOWTO](docs/HOWTO.md)