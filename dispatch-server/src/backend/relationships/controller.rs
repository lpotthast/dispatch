//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct RelationshipController {
    pub(super) relationships: Arc<crate::backend::relationships::service::RelationshipService>,
}
impl RelationshipController {
    pub(crate) fn new(
        relationships: Arc<crate::backend::relationships::service::RelationshipService>,
    ) -> Self {
        Self { relationships }
    }
}
