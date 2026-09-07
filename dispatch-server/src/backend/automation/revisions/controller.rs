//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct RevisionController {
    pub(super) revision_queries:
        Arc<crate::backend::automation::revisions::service::RevisionQueryService>,
}
impl RevisionController {
    pub(crate) fn new(
        revision_queries: Arc<crate::backend::automation::revisions::service::RevisionQueryService>,
    ) -> Self {
        Self { revision_queries }
    }
}
