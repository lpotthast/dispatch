//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct KnowledgeJobController {
    pub(super) jobs: Arc<crate::backend::knowledge::jobs::service::JobService>,
}
impl KnowledgeJobController {
    pub(crate) fn new(jobs: Arc<crate::backend::knowledge::jobs::service::JobService>) -> Self {
        Self { jobs }
    }
}
