//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct RunController {
    pub(super) run_control: Arc<crate::backend::runs::control::RunControlService>,
    pub(super) run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
}
impl RunController {
    pub(crate) fn new(
        run_control: Arc<crate::backend::runs::control::RunControlService>,
        run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
    ) -> Self {
        Self {
            run_control,
            run_queries,
        }
    }
}
