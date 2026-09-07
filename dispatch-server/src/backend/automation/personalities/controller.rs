//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct PersonalityController {
    pub(super) personalities:
        Arc<crate::backend::automation::personalities::service::PersonalityService>,
}
impl PersonalityController {
    pub(crate) fn new(
        personalities: Arc<crate::backend::automation::personalities::service::PersonalityService>,
    ) -> Self {
        Self { personalities }
    }
}
