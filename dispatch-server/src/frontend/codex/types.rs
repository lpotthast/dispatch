use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct CodexReadiness {
    pub(crate) selected_project: Option<String>,
    pub(crate) codex_status: CodexAppServerStatusView,
}
impl From<CodexStatusPage> for CodexReadiness {
    fn from(value: CodexStatusPage) -> Self {
        Self {
            selected_project: value.selected_project,
            codex_status: value.codex_status,
        }
    }
}
