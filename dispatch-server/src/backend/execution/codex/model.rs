use crate::shared::view_models::CodexAppServerStatusView;
use std::sync::Arc;
use tokio::sync::RwLock;
pub(crate) type SharedCodexStatus = Arc<RwLock<CodexAppServerStatusView>>;
