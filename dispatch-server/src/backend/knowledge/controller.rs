//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct KnowledgeController {
    pub(super) knowledge_queries: Arc<crate::backend::knowledge::queries::KnowledgeQueryService>,
}
impl KnowledgeController {
    pub(crate) fn new(
        knowledge_queries: Arc<crate::backend::knowledge::queries::KnowledgeQueryService>,
    ) -> Self {
        Self { knowledge_queries }
    }
}
