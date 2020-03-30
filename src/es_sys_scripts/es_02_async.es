
// timeout / immeditate for async dev

esses.async = new (class EssesAsync {

    constructor(){
        this.immediates = new Map();
        this.immediate_todos = [];
        this._runningImmediate = false;
    }

    immediate(f, ...args) {
        let id = esses.next_id();
        console.trace("registering immediate with id {} in runtime {}", id, esses._runtime_id);
        this.immediates.set(id, {f, args});
        if (this._runningImmediate) {
            this.immediate_todos.push(id);
        } else {
            esses.invoke_rust_op_sync("sched_immediate", id);
        }

        return id;
    }

    _run_immediate_todos() {
        if (this.immediate_todos.length > 0) {
            this._run_immediate_from_rust(this.immediate_todos.shift());
        }
    }

    _run_immediate_from_rust(id) {
        this._runningImmediate = true;
        try {
            let i = this.immediates.get(id);
            console.trace("_run_immediate_from_rust with id {} i={}", id, typeof i);
            this.immediates.delete(id);
            try {
                console.trace("_run_immediate_from_rust with id {}", id);
                i.f.apply(null, i.args);
            } catch(ex){
                console.debug("_run_immediate_from_rust {} failed: {}", id, ex);
                throw ex;
            }
        } finally {
            this._runningImmediate = false;
            this._run_immediate_todos();
        }
    }

})();

this.setImmediate = function(f, ...args) {
    return esses.async.immediate(f, ...args);
}



