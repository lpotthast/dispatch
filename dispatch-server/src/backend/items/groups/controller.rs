//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct GroupController {
    pub(super) groups: Arc<crate::backend::items::groups::service::GroupService>,
}
impl GroupController {
    pub(crate) fn new(groups: Arc<crate::backend::items::groups::service::GroupService>) -> Self {
        Self { groups }
    }
}
