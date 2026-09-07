//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct AutomationController {
    pub(super) automation_supervisor: crate::backend::automation::supervisor::AutomationSupervisor,
    pub(super) claims: Arc<crate::backend::items::claims::service::ClaimService>,
    pub(super) launch: Arc<crate::backend::automation::launch::service::LaunchService>,
}
impl AutomationController {
    pub(crate) fn new(
        automation_supervisor: crate::backend::automation::supervisor::AutomationSupervisor,
        claims: Arc<crate::backend::items::claims::service::ClaimService>,
        launch: Arc<crate::backend::automation::launch::service::LaunchService>,
    ) -> Self {
        Self {
            automation_supervisor,
            claims,
            launch,
        }
    }
}
