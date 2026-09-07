//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct WorkspaceController {
    pub(super) workspaces: Arc<crate::backend::execution::workspaces::service::WorkspaceService>,
}
impl WorkspaceController {
    pub(crate) fn new(
        workspaces: Arc<crate::backend::execution::workspaces::service::WorkspaceService>,
    ) -> Self {
        Self { workspaces }
    }
}
