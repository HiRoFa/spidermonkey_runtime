use crate::esruntimewrapper::EsRuntimeWrapper;

pub(crate) fn init_es(rt: &EsRuntimeWrapper) {
    init_file(
        rt,
        "es_sys_scripts/es_01_core.es",
        include_str!("es_sys_scripts/es_01_core.es"),
    );

    init_file(
        rt,
        "es_sys_scripts/es_02_async.es",
        include_str!("es_sys_scripts/es_02_async.es"),
    );

    init_file(
        rt,
        "es_sys_scripts/es_03_promises.es",
        include_str!("es_sys_scripts/es_03_promises.es"),
    );
}

fn init_file(runtime: &EsRuntimeWrapper, file_name: &str, es_code: &str) {
    let init_res = runtime.eval_void_sync(es_code, file_name);
    if !init_res.is_ok() {
        let esei = init_res.err().unwrap();
        panic!(
            "could not init file: {} at {}:{}:{} ",
            esei.message, esei.filename, esei.lineno, esei.column
        );
    }
}
