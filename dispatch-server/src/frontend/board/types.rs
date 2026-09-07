use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BoardShell {
    pub(crate) selected_project: Option<String>,
    pub(crate) selected_project_view: Option<ProjectView>,
    pub(crate) automation_status: Option<AutomationStatusView>,
    pub(crate) automation_running: bool,
    pub(crate) label_suggestions: Vec<ProjectLabelView>,
}
impl From<BoardPage> for BoardShell {
    fn from(value: BoardPage) -> Self {
        Self {
            selected_project: value.selected_project,
            selected_project_view: value.selected_project_view,
            automation_status: value.automation_status,
            automation_running: value.automation_running,
            label_suggestions: value.label_suggestions,
        }
    }
}
