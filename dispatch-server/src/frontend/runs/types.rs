use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RunDetail {
    pub(crate) project: String,
    pub(crate) run_log: RunLogView,
}
impl From<RunLogPage> for RunDetail {
    fn from(value: RunLogPage) -> Self {
        Self {
            project: value.project,
            run_log: value.run_log,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunOutputEntry {
    pub(crate) start: Option<AgentRunOutputPiece>,
    pub(crate) piece: AgentRunOutputPiece,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandPresentation {
    pub(crate) display: String,
    pub(crate) exploring_file: Option<String>,
}
