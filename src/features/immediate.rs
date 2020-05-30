use log::debug;

use crate::esruntimewrapper::EsRuntimeWrapper;
use crate::esruntimewrapperinner::EsRuntimeWrapperInner;
use crate::esvaluefacade::EsValueFacade;
use std::sync::Arc;

pub(crate) fn init(rt: &EsRuntimeWrapper) {
    rt.register_op(
        "sched_immediate",
        Arc::new(|rt: &EsRuntimeWrapperInner, args: Vec<EsValueFacade>| {
            debug!(
                "running op sched_immediate in rust with rt with {} args",
                args.len()
            );

            let id_arg = args.get(0).expect("did not get enough args");

            let id = *id_arg.get_i32();

            debug!(
                "running op sched_immediate in rust with rt with {} args. id={}",
                args.len(),
                id
            );

            rt.call(
                // todo natively do this
                vec!["esses", "async"],
                "_run_immediate_from_rust",
                vec![EsValueFacade::new_i32(id)],
            );

            debug!("done running sched immediate in rust with id {}", id);

            Ok(EsValueFacade::undefined())
        }),
    );
}
