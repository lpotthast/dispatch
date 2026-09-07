use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ItemDetail {
    pub(crate) project: String,
    pub(crate) item: WorkItemView,
    pub(crate) comments: Vec<CommentView>,
    pub(crate) relationships: Vec<WorkItemRelationshipListEntry>,
    pub(crate) label_suggestions: Vec<ProjectLabelView>,
    pub(crate) work_item_states: Vec<WorkItemStateView>,
    pub(crate) automation_runs: Vec<AgentRunView>,
}
impl From<ItemPage> for ItemDetail {
    fn from(value: ItemPage) -> Self {
        Self {
            project: value.project,
            item: value.item,
            comments: value.comments,
            relationships: value.relationships,
            label_suggestions: value.label_suggestions,
            work_item_states: value.work_item_states,
            automation_runs: value.automation_runs,
        }
    }
}
