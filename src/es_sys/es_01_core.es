

this.esses = new (class Esses {

    constructor() {

        this._next_id = 0;
        this._cleanup_jobs = [];

    }

    /**
    * Promises are returned in the form of {"__esses_future_obj_id": 12, "runtime_id": 4}
    *
    */
    prepValForOutputToRust(val) {
        // for now we only store Promises as managed, all other objects are returned by ref and serialized in rust
        // if you want it to behave differently, reg the var yourself from js
        if (typeof val === 'object' && val instanceof Promise) {
            let id = this.next_id();
            let prom = val;
            val = {"__esses_future_obj_id": id};
            setImmediate(function(prom, id) {
                esses.register_waitfor_promise(prom, id);
            }, prom, id);
        }
        return val;
    }

    next_id() {
        return this._next_id++;
    }

    /**
    * currently this runs in the same threadpool as all jobs for this runtime but in future this will become a multithreaded pool
    * @returns {Promise}
    */
    invoke_rust_op(name, ...args) {

        return new Promise((resolve, reject) => {
            try {
                let res = this.invoke_rust_op_sync(name, args);
                resolve(res);
            } catch(ex) {
                reject(ex);
            }
        });

    }

    /**
    * @returns {Void}
    */
    invoke_rust_op_void(name, ...args) {

        setImmediate(() => {
            this.invoke_rust_op_sync(name, args);
        });

    }

    /**
    * @returns {Any}
    */
    invoke_rust_op_sync(name, ...args) {

        console.log("invoke_rust_op_sync %s ", name);
        try {
            let prepped_args = args.map((arg) => this.prepValForOutputToRust(arg));
            let rust_result = __invoke_rust_op(name, ...prepped_args);
            return rust_result;
        } catch(ex) {
            console.error("invoke_rust_op_sync %s failed with %s", name, "" + ex);
        }

    }

    register_waitfor_promise(val, man_obj_id) {

        if (val instanceof Promise) {
            val.then((result) => {
                console.trace('resolving esvf from es to {}', result);
                esses.invoke_rust_op_sync('resolve_waiting_esvf_future', man_obj_id, result);
            });
            val.catch((ex) => {
                console.trace('rejecting esvf from es to {}', ex);
                esses.invoke_rust_op_sync('reject_waiting_esvf_future', man_obj_id, ex);
            });
        } else {
            let t = "" + val;
            if (val && val.constructor) {
                t = val.constructor.name;
            } else if (val){
                t = JSON.stringify(val);
            }
            throw Error("_register_waitfor_promise_ managed obj was not a promise: " + t);
        }
    }

    /**
    * add a job todo when cleanup is called from rust
    */
    add_cleanup_job(job) {
        if (!job instanceof Function) {
            throw Error("job was not a function");
        }
        this._cleanup_jobs.push(job);
    }

    /**
    * called from rust before running cleanup
    */
    cleanup() {
        console.debug("running esses.cleanup()");
        for (let job of this._cleanup_jobs) {
            job();
        }
    }


})();

this._esses_cleanup = function(){
    return esses.cleanup();
};

