use crate::esruntime::EsRuntime;

pub(crate) fn init_es(rt: &EsRuntime) {
    init_file(
        rt,
        "es_sys_scripts/es_01_core.es",
        include_str!("es_sys_scripts/es_01_core.es"),
    );
}

fn init_file(runtime: &EsRuntime, file_name: &str, es_code: &str) {
    let init_res = runtime.eval_void_sync(es_code, file_name);
    if init_res.is_err() {
        let esei = init_res.err().unwrap();
        panic!(
            "could not init file: {} at {}:{}:{} ",
            esei.message, esei.filename, esei.lineno, esei.column
        );
    }
}
