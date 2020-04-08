use crate::microtaskmanager::MicroTaskManager;
use mozjs::rust::{JSEngine, JSEngineHandle};
use std::cell::RefCell;
use std::sync::Arc;

// this class has a single taskmanager which has a single thread which initializes and destroys the JSEngine
// one might argue that you should just do this from your main thread but that does not work with tests

thread_local! {
    static ENGINE: RefCell<JSEngine> = RefCell::new(JSEngine::init().unwrap());
}

lazy_static! {
    static ref TASKMANAGER: Arc<MicroTaskManager> = MicroTaskManager::new();
}

pub fn produce() -> JSEngineHandle {
    TASKMANAGER.clone().exe_task(|| {
        ENGINE.with(|engine_rc| {
            let engine: &JSEngine = &*engine_rc.borrow();
            engine.handle()
        })
    })
}
