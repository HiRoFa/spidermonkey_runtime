use hirofa_utils::eventloop::EventLoop;
use mozjs::rust::{JSEngine, JSEngineHandle};
use std::cell::RefCell;

// this class has a single taskmanager which has a single thread which initializes and destroys the JSEngine
// one might argue that you should just do this from your main thread but that does not work with tests

thread_local! {
    static ENGINE: RefCell<JSEngine> = RefCell::new(JSEngine::init().unwrap());
}

lazy_static! {
    static ref EVENTLOOP: EventLoop = EventLoop::new();
}

pub fn produce() -> JSEngineHandle {
    EVENTLOOP.exe(|| {
        ENGINE.with(|engine_rc| {
            let engine: &JSEngine = &*engine_rc.borrow();
            engine.handle()
        })
    })
}
