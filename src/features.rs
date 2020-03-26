use crate::esruntimewrapper::EsRuntimeWrapper;

/// features add a piece of functionality to the engine
/// they may add a native method, a rust op or complete scripts
mod console;
mod esvf;
mod immediate;

pub(crate) fn init(rt: &EsRuntimeWrapper) {
    immediate::init(rt);
    esvf::init(rt);
    console::init(rt);
}
