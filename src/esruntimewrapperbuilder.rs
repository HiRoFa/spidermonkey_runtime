use crate::esruntimewrapper::{EsRuntimeWrapper, ModuleCodeLoader};
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use std::time::Duration;

pub struct EsRuntimeWrapperBuilder {
    gc_interval: Option<Duration>,
    pub(crate) module_code_loader: Option<Box<ModuleCodeLoader>>,
    pub(crate) module_cache_size: usize,
    built: bool,
}

impl EsRuntimeWrapperBuilder {
    pub fn new() -> Self {
        EsRuntimeWrapperBuilder {
            gc_interval: None,
            module_code_loader: None,
            module_cache_size: 50,
            built: false,
        }
    }

    pub fn gc_interval(&mut self, interval: Duration) -> &mut Self {
        self.gc_interval = Some(interval);
        self
    }

    pub fn module_code_loader(&mut self, loader: Box<ModuleCodeLoader>) -> &mut Self {
        self.module_code_loader = Some(loader);
        self
    }

    pub fn module_cache_size(&mut self, size: usize) -> &mut Self {
        self.module_cache_size = size;
        self
    }

    pub fn build(&mut self) -> EsRuntimeWrapper {
        if self.built {
            panic!("cannot reuse builder");
        }

        self.built = true;

        // consume opts

        let mut mcl_opt: Option<Box<ModuleCodeLoader>> = None;
        if self.module_code_loader.is_some() {
            let cl: Option<Box<ModuleCodeLoader>> =
                std::mem::replace(&mut self.module_code_loader, None);
            mcl_opt = cl;
        }

        let inner = EsRuntimeWrapperInner::build(mcl_opt, self.module_cache_size);
        let wrapper = EsRuntimeWrapper::new_inner(inner);
        if self.gc_interval.is_some() {
            wrapper.start_gc_deamon(self.gc_interval.unwrap());
        }
        wrapper
    }
}
