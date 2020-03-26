use log::{debug, trace};

use crate::esruntimewrapper::EsRuntimeWrapper;
use crate::esvaluefacade::EsValueFacade;
use crate::spidermonkeyruntimewrapper::SmRuntime;

pub(crate) fn init(rt: &EsRuntimeWrapper) {
    rt.register_op(
        "resolve_waiting_esvf_future",
        Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
            let mut args = args;
            debug!(
                "running op resolve_waiting_esvf_future in rust with rt with {} args",
                args.len()
            );

            let result_arg = args.remove(1);
            let id_arg = args.get(0).expect("did not get enough args");

            let man_obj_id: i32 = id_arg.get_i32().clone();

            trace!(
                "resolving future from promise from esvf man_obj_id:{}",
                man_obj_id
            );
            let fut_res: Result<EsValueFacade, EsValueFacade> = Ok(result_arg);

            EsValueFacade::resolve_future(man_obj_id, fut_res);

            Ok(EsValueFacade::undefined())
        }),
    );

    rt.register_op(
        "reject_waiting_esvf_future",
        Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
            let mut args = args;
            debug!(
                "running op reject_waiting_esvf_future in rust with rt with {} args",
                args.len()
            );

            let result_arg = args.remove(1);
            let id_arg = args.get(0).expect("did not get enough args");

            let man_obj_id: i32 = id_arg.get_i32().clone();

            trace!(
                "rejecting future from promise from esvf man_obj_id:{}",
                man_obj_id
            );
            let fut_res: Result<EsValueFacade, EsValueFacade> = Err(result_arg);

            EsValueFacade::resolve_future(man_obj_id, fut_res);

            Ok(EsValueFacade::undefined())
        }),
    );
}
