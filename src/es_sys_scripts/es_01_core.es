

this.esses = new (class Esses {

    constructor() {

        this._next_id = 0;
        this._cleanup_jobs = [];
        this._registered_promises = new Map();
        this._runtime_id = Math.floor(Math.random() * 10000);

    }

    next_id() {
        return this._next_id++;
    }

    /**
    * @returns {Promise}
    */
    invoke_rust_op(name, ...args) {

        console.log("invoke_rust_op %s ", name);
        try {
            let rust_result = __invoke_rust_op(name, ...args);
            return rust_result;
        } catch(ex) {
            console.error("invoke_rust_op %s failed with %s", name, "" + ex);
            throw ex;
        }

    }

    /**
    * @returns {Void}
    */
    invoke_rust_op_void(name, ...args) {

        setImmediate(() => {
            this.invoke_rust_op_sync(name, ...args);
        });

    }

    /**
    * @returns {Any}
    */
    invoke_rust_op_sync(name, ...args) {

        console.log("invoke_rust_op_sync %s ", name);
        try {
            let rust_result = __invoke_rust_op_sync(name, ...args);
            return rust_result;
        } catch(ex) {
            console.error("invoke_rust_op_sync %s failed with %s", name, "" + ex);
            throw ex;
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

