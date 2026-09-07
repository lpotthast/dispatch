//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct CodexController {
    pub(super) codex: Arc<crate::backend::execution::codex::service::CodexService>,
    pub(super) codex_status:
        Arc<tokio::sync::RwLock<crate::shared::view_models::CodexAppServerStatusView>>,
}
impl CodexController {
    pub(crate) fn new(
        codex: Arc<crate::backend::execution::codex::service::CodexService>,
        codex_status: Arc<
            tokio::sync::RwLock<crate::shared::view_models::CodexAppServerStatusView>,
        >,
    ) -> Self {
        Self {
            codex,
            codex_status,
        }
    }
}
