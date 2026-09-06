use serde::{Deserialize, Serialize};

use crate::{AgentReasoningEffort, AuthorType, WorkItemRelationshipView, WorkItemView};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateWorkItemRequest {
    pub title: String,
    pub description: String,
    pub state: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "labels")]
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkItemGroupRequest {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignWorkItemGroupRequest {
    pub item_ids: Vec<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateWorkItemRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub state: Option<String>,
    pub agent_model_override: Option<Option<String>>,
    pub agent_reasoning_effort_override: Option<Option<AgentReasoningEffort>>,
    pub expect_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClaimWorkItemRequest {
    pub agent_id: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClaimWorkItemResponse {
    pub item: Option<WorkItemView>,
}

impl ClaimWorkItemResponse {
    pub fn claimed(&self) -> bool {
        self.item.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateWorkItemLabelRequest {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateWorkItemLabelRequest {
    pub key: Option<String>,
    pub value: Option<Option<String>>,
    pub expect_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteWorkItemLabelResponse {
    pub deleted: bool,
    pub label_id: i64,
    pub work_item: WorkItemView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateWorkItemRelationshipRequest {
    pub target_work_item_id: i64,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateWorkItemRelationshipRequest {
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteWorkItemRelationshipResponse {
    pub deleted: bool,
    pub relationship: WorkItemRelationshipView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProgressWorkItemRequest {
    pub agent_id: String,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FinishWorkItemRequest {
    pub agent_id: String,
    pub report: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReleaseWorkItemRequest {
    pub agent_id: String,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequestFeedbackWorkItemRequest {
    pub agent_id: String,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AddCommentRequest {
    pub author_type: AuthorType,
    pub author_name: Option<String>,
    pub body: String,
}
