use std::{fmt, str::FromStr};

use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use serde::{Deserialize, Serialize};

use crate::{AgentReasoningEffort, ParseEnumError, WorkItemOriginView};

pub const STATE_LABEL_KEY: &str = "state";
pub const DEFAULT_STATE_LABEL: &str = "open";
pub const CLAIMED_STATE_LABEL: &str = "in_progress";
pub const FINISHED_STATE_LABEL: &str = "done";
pub const CLAIMED_FROM_STATE_LABEL_KEY: &str = "dispatch:claimed-from-state";
pub const AUTOMATION_BLOCKED_LABEL_KEY: &str = "dispatch:automation-blocked";
pub const FEEDBACK_REQUESTED_LABEL_KEY: &str = "dispatch:feedback-requested";
pub const NEEDS_REFINEMENT_LABEL_KEY: &str = "needs-refinement";
pub const NEEDS_VERIFICATION_LABEL_KEY: &str = "needs-verification";

pub fn default_automation_work_item_selector() -> Condition {
    Condition::All(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String(DEFAULT_STATE_LABEL.to_owned()),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: NEEDS_REFINEMENT_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: NEEDS_VERIFICATION_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
    ])
}

pub fn needs_refinement_automation_work_item_selector() -> Condition {
    label_presence_selector(NEEDS_REFINEMENT_LABEL_KEY)
}

pub fn needs_verification_automation_work_item_selector() -> Condition {
    label_presence_selector(NEEDS_VERIFICATION_LABEL_KEY)
}

fn label_presence_selector(label_key: &str) -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: label_key.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::Bool(true),
    })])
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemView {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub description: String,
    pub state: Option<String>,
    pub labels: Vec<WorkItemLabelView>,
    pub version: i64,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub claim_expires_at: Option<String>,
    pub claim_source: Option<WorkItemClaimSourceView>,
    pub finished_at: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_group: Option<WorkItemGroupSummaryView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<WorkItemOriginView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardWorkItemView {
    pub id: i64,
    pub title: String,
    pub description_excerpt: String,
    pub labels: Vec<WorkItemLabelView>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub claim_source: Option<WorkItemClaimSourceView>,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: i64,
    pub work_group: Option<WorkItemGroupSummaryView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemGroupSummaryView {
    pub id: i64,
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemGroupView {
    pub id: i64,
    pub project_id: i64,
    pub key: String,
    pub name: String,
    pub item_count: u64,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemClaimSourceView {
    pub run_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemLabelView {
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: i64,
    pub key: String,
    pub value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemRelationshipDirection {
    Outgoing,
    Incoming,
}

impl WorkItemRelationshipDirection {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
        }
    }
}

impl fmt::Display for WorkItemRelationshipDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipItemSummary {
    pub id: i64,
    pub title: String,
    pub state: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipView {
    pub id: i64,
    pub project_id: i64,
    pub kind: String,
    pub source_work_item_id: i64,
    pub target_work_item_id: i64,
    pub source: WorkItemRelationshipItemSummary,
    pub target: WorkItemRelationshipItemSummary,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipListEntry {
    pub relationship: WorkItemRelationshipView,
    pub direction: WorkItemRelationshipDirection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectLabelView {
    pub key: String,
    pub value: Option<String>,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
}

/// Supported ordering strategies for work items inside a swim-lane.
///
/// The enum is shared by the server and hydrated frontend so persisted lane configuration is
/// validated once and board rendering remains exhaustive when a new ordering mode is added.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwimLaneItemOrder {
    #[default]
    UpdatedDesc,
    UpdatedAsc,
    CreatedDesc,
    CreatedAsc,
    IdDesc,
    IdAsc,
    TitleAsc,
    TitleDesc,
}

impl SwimLaneItemOrder {
    /// Returns the canonical SQLite and JSON representation.
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "updated_desc",
            Self::UpdatedAsc => "updated_asc",
            Self::CreatedDesc => "created_desc",
            Self::CreatedAsc => "created_asc",
            Self::IdDesc => "id_desc",
            Self::IdAsc => "id_asc",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
        }
    }
}

impl fmt::Display for SwimLaneItemOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for SwimLaneItemOrder {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "updated_desc" => Ok(Self::UpdatedDesc),
            "updated_asc" => Ok(Self::UpdatedAsc),
            "created_desc" => Ok(Self::CreatedDesc),
            "created_asc" => Ok(Self::CreatedAsc),
            "id_desc" => Ok(Self::IdDesc),
            "id_asc" => Ok(Self::IdAsc),
            "title_asc" => Ok(Self::TitleAsc),
            "title_desc" => Ok(Self::TitleDesc),
            _ => Err(ParseEnumError(
                "swim-lane item order must be one of: updated_desc, updated_asc, created_desc, created_asc, id_desc, id_asc, title_asc, title_desc",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SwimLaneView {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub filter: Condition,
    pub item_order: SwimLaneItemOrder,
    pub can_create_items: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemStateView {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A durable event kind in Dispatch's project-scoped workflow audit stream.
///
/// Storage uses historical spellings for the two project snapshot events and snake case for item
/// workflow events. Keeping those spellings behind this enum prevents producers from inventing
/// event names while preserving the existing API and database representation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemEventType {
    #[serde(rename = "SystemPromptChanged")]
    SystemPromptChanged,
    #[serde(rename = "MemoryChanged")]
    MemoryChanged,
    ItemCreated,
    ItemUpdated,
    ItemMoved,
    ItemDeleted,
    ItemClaimed,
    ProgressAdded,
    ItemFinished,
    ItemReleased,
    FeedbackRequested,
    CommentAdded,
    LabelAdded,
    LabelUpdated,
    LabelDeleted,
    RelationshipCreated,
    RelationshipUpdated,
    RelationshipDeleted,
}

impl WorkItemEventType {
    /// Returns the canonical SQLite and server-sent-event representation.
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::SystemPromptChanged => "SystemPromptChanged",
            Self::MemoryChanged => "MemoryChanged",
            Self::ItemCreated => "item_created",
            Self::ItemUpdated => "item_updated",
            Self::ItemMoved => "item_moved",
            Self::ItemDeleted => "item_deleted",
            Self::ItemClaimed => "item_claimed",
            Self::ProgressAdded => "progress_added",
            Self::ItemFinished => "item_finished",
            Self::ItemReleased => "item_released",
            Self::FeedbackRequested => "feedback_requested",
            Self::CommentAdded => "comment_added",
            Self::LabelAdded => "label_added",
            Self::LabelUpdated => "label_updated",
            Self::LabelDeleted => "label_deleted",
            Self::RelationshipCreated => "relationship_created",
            Self::RelationshipUpdated => "relationship_updated",
            Self::RelationshipDeleted => "relationship_deleted",
        }
    }
}

impl fmt::Display for WorkItemEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for WorkItemEventType {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "SystemPromptChanged" => Ok(Self::SystemPromptChanged),
            "MemoryChanged" => Ok(Self::MemoryChanged),
            "item_created" => Ok(Self::ItemCreated),
            "item_updated" => Ok(Self::ItemUpdated),
            "item_moved" => Ok(Self::ItemMoved),
            "item_deleted" => Ok(Self::ItemDeleted),
            "item_claimed" => Ok(Self::ItemClaimed),
            "progress_added" => Ok(Self::ProgressAdded),
            "item_finished" => Ok(Self::ItemFinished),
            "item_released" => Ok(Self::ItemReleased),
            "feedback_requested" => Ok(Self::FeedbackRequested),
            "comment_added" => Ok(Self::CommentAdded),
            "label_added" => Ok(Self::LabelAdded),
            "label_updated" => Ok(Self::LabelUpdated),
            "label_deleted" => Ok(Self::LabelDeleted),
            "relationship_created" => Ok(Self::RelationshipCreated),
            "relationship_updated" => Ok(Self::RelationshipUpdated),
            "relationship_deleted" => Ok(Self::RelationshipDeleted),
            _ => Err(ParseEnumError("unknown work item event type")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItemEventView {
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: Option<i64>,
    pub event_type: WorkItemEventType,
    pub body: String,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecoveredClaimView {
    pub item_id: i64,
    pub agent_id: String,
    pub claimed_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorType {
    User,
    Agent,
    System,
}

impl AuthorType {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }
}

impl fmt::Display for AuthorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AuthorType {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            "system" => Ok(Self::System),
            _ => Err(ParseEnumError(
                "author type must be one of: user, agent, system",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommentView {
    pub id: i64,
    pub work_item_id: i64,
    pub author_type: AuthorType,
    pub author_name: Option<String>,
    pub body: String,
    pub created_at: String,
}
