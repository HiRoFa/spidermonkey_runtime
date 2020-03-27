use log::debug;

use crate::esruntimewrapper::EsRuntimeWrapper;
use crate::esvaluefacade::EsValueFacade;
use crate::spidermonkeyruntimewrapper::SmRuntime;

pub(crate) fn init(rt: &EsRuntimeWrapper) {
    rt.register_op(
        "sched_immediate",
        Box::new(|sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
            debug!(
                "running op sched_immediate in rust with rt with {} args",
                args.len()
            );

            let id_arg = args.get(0).expect("did not get enough args");

            let id = id_arg.get_i32().clone();

            debug!(
                "running op sched_immediate in rust with rt with {} args. id={}",
                args.len(),
                id
            );

            // todo pass runtime or runtimeid as param for every OP
            //rt.eval(code);

            sm_rt.do_with_sm_rt_async(move |sm_rt| {
                sm_rt
                    .call(vec!["esses", "async"], "_run_immediate_from_rust", vec![EsValueFacade::new_i32(id)])
                    .ok()
                    .expect("could not invoke _run_immediate_from_rust");
            });

            debug!("done running sched immediate in rust with id {}", id);

            Ok(EsValueFacade::undefined())
        }),
    );
}
