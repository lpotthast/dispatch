//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct ClaimController {
    pub(super) claims: Arc<crate::backend::items::claims::service::ClaimService>,
}
impl ClaimController {
    pub(crate) fn new(claims: Arc<crate::backend::items::claims::service::ClaimService>) -> Self {
        Self { claims }
    }
}
