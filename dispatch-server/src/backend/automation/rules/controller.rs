//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct RuleController {
    pub(super) attribution: Arc<crate::backend::attribution::service::AttributionService>,
    pub(super) rules: Arc<crate::backend::automation::rules::service::RuleService>,
}
impl RuleController {
    pub(crate) fn new(
        attribution: Arc<crate::backend::attribution::service::AttributionService>,
        rules: Arc<crate::backend::automation::rules::service::RuleService>,
    ) -> Self {
        Self { attribution, rules }
    }
}
