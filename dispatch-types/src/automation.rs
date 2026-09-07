use crate::{CodexAppServerStatusView, ProjectView};
use std::{fmt, str::FromStr};

use crudkit_core::condition::Condition;
use serde::{Deserialize, Serialize};

use crate::{
    AgentReasoningEffort, AgentRunOutputPiece, AgentRunView, AgentToolName, AgentToolView,
    AuthorType, AutomationRunMutability, CreateWorkItemLabelRequest, DEFAULT_STATE_LABEL,
    ParseEnumError, ProjectSettingsView, WorkItemEventType, WorkItemView, optional_condition,
};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PersonalityView {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub personality_description: String,
    #[serde(default)]
    pub current_revision_id: Option<i64>,
    #[serde(default)]
    pub managed_bundle_key: Option<String>,
    #[serde(default)]
    pub managed_object_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AutomationStatusView {
    pub project: String,
    pub settings: ProjectSettingsView,
    pub running_runs: i64,
    pub running_mutating_runs: i64,
    pub running_read_only_runs: i64,
    pub allowed_mutating_runs: i64,
    pub recent_runs: Vec<AgentRunView>,
    pub tools: Vec<AgentToolView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationActivation {
    Manual,
    WorkItem,
    Cron,
    WorkItemCreated,
}

impl AutomationActivation {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::WorkItem => "work_item",
            Self::Cron => "cron",
            Self::WorkItemCreated => "work_item_created",
        }
    }
}

impl fmt::Display for AutomationActivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AutomationActivation {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "manual" => Ok(Self::Manual),
            "work_item" | "started" | "start" => Ok(Self::WorkItem),
            "cron" => Ok(Self::Cron),
            "work_item_created" => Ok(Self::WorkItemCreated),
            _ => Err(ParseEnumError(
                "automation activation must be one of: manual, work_item, cron, work_item_created",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEffect {
    ProduceWork,
    ConsumeWork,
}

impl AutomationEffect {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::ProduceWork => "produce_work",
            Self::ConsumeWork => "consume_work",
        }
    }
}

impl fmt::Display for AutomationEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AutomationEffect {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "produce_work" | "produce" | "producer" => Ok(Self::ProduceWork),
            "consume_work" | "consume" | "consumer" => Ok(Self::ConsumeWork),
            _ => Err(ParseEnumError(
                "automation effect must be one of: produce_work, consume_work",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AutomationTriggerView {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub enabled: bool,
    #[serde(alias = "trigger_kind")]
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub tool_name: AgentToolName,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub personality_name: Option<String>,
    pub prompt: String,
    #[serde(default, with = "optional_condition")]
    pub work_item_selector: Option<Condition>,
    pub priority: i64,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub produced_work: Option<ProducedWorkSpec>,
    #[serde(default)]
    pub execution: AutomationExecutionPolicy,
    #[serde(default)]
    pub postconditions: Option<AutomationPostconditions>,
    #[serde(default)]
    pub current_revision_id: Option<i64>,
    #[serde(default)]
    pub managed_bundle_key: Option<String>,
    #[serde(default)]
    pub managed_object_key: Option<String>,
    pub evaluation_count: i64,
    pub pending_evaluation_count: i64,
    pub last_evaluation_queued_at: Option<String>,
    pub last_evaluated_at: Option<String>,
    pub next_evaluation_at: Option<String>,
    pub last_event_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_produced_work_state() -> String {
    DEFAULT_STATE_LABEL.to_owned()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProducedWorkSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_produced_work_state")]
    pub state: String,
    #[serde(default)]
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
    #[serde(default)]
    pub agent_model_override: Option<String>,
    #[serde(default)]
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    #[serde(default)]
    pub deduplication: ProduceDeduplication,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "policy", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProduceDeduplication {
    #[default]
    Always,
    WhileUnfinishedForTrigger,
    WhileUnfinishedForKey {
        key: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationExecutionPolicy {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<AgentReasoningEffort>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_concurrent_runs: Option<u64>,
    #[serde(default)]
    pub concurrency_group: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPostconditions {
    #[serde(default)]
    pub any_of: Vec<AutomationOutcomeSet>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationOutcomeSet {
    #[serde(default)]
    pub disposition: Option<ExpectedDisposition>,
    #[serde(default)]
    pub attributed_events: Vec<WorkItemEventType>,
    #[serde(default)]
    pub labels: Vec<LabelAssertion>,
    #[serde(default)]
    pub created_items: Option<CreatedItemAssertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_item_assertions: Vec<CreatedItemAssertion>,
    #[serde(default)]
    pub created_items_share_group: bool,
    #[serde(default)]
    pub workspace_changes: Option<WorkspaceAssertion>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedDisposition {
    Finished,
    Released,
    FeedbackRequested,
    SuccessfulNonterminal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabelAssertion {
    pub assertion: LabelAssertionKind,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelAssertionKind {
    Added,
    Removed,
    Present,
    Absent,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedItemAssertion {
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub at_least: Option<u64>,
    #[serde(default)]
    pub at_most: Option<u64>,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAssertion {
    Any,
    None,
    Required,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPostconditionStatus {
    #[default]
    NotConfigured,
    Passed,
    Failed,
}

impl SemanticPostconditionStatus {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for SemanticPostconditionStatus {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "not_configured" => Ok(Self::NotConfigured),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseEnumError(
                "semantic postcondition status must be one of: not_configured, passed, failed",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PostconditionFailureView {
    pub outcome_index: usize,
    pub assertion: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemOriginKind {
    Historical,
    Operator,
    ProducingAutomation,
    AgentRun,
    System,
}

impl WorkItemOriginKind {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::Operator => "operator",
            Self::ProducingAutomation => "producing_automation",
            Self::AgentRun => "agent_run",
            Self::System => "system",
        }
    }
}

impl FromStr for WorkItemOriginKind {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "historical" => Ok(Self::Historical),
            "operator" => Ok(Self::Operator),
            "producing_automation" => Ok(Self::ProducingAutomation),
            "agent_run" => Ok(Self::AgentRun),
            "system" => Ok(Self::System),
            _ => Err(ParseEnumError("unknown work item origin kind")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemOriginView {
    pub kind: WorkItemOriginKind,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub producing_evaluation_id: Option<i64>,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: Option<String>,
    pub bundle_key: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemSummaryView {
    pub id: i64,
    pub title: String,
    pub state: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionChangeOperation {
    Create,
    Update,
    Restore,
    Detach,
    BundleApply,
    Migration,
}

impl RevisionChangeOperation {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Restore => "restore",
            Self::Detach => "detach",
            Self::BundleApply => "bundle_apply",
            Self::Migration => "migration",
        }
    }
}

impl FromStr for RevisionChangeOperation {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "create" => Ok(Self::Create),
            "update" => Ok(Self::Update),
            "restore" => Ok(Self::Restore),
            "detach" => Ok(Self::Detach),
            "bundle_apply" => Ok(Self::BundleApply),
            "migration" => Ok(Self::Migration),
            _ => Err(ParseEnumError("unknown revision change operation")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AutomationRevisionView {
    pub id: i64,
    pub trigger_id: Option<i64>,
    pub project_id: i64,
    pub revision_number: i64,
    pub configuration: serde_json::Value,
    pub sha256: String,
    pub operation: RevisionChangeOperation,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersonalityRevisionView {
    pub id: i64,
    pub personality_id: Option<i64>,
    pub project_id: i64,
    pub revision_number: i64,
    pub name: String,
    pub personality_description: String,
    pub sha256: String,
    pub operation: RevisionChangeOperation,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEvaluationOutcome {
    CreatedWork,
    SkippedDuplicate,
    StartedRun,
    Failed,
}

impl AutomationEvaluationOutcome {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::CreatedWork => "created_work",
            Self::SkippedDuplicate => "skipped_duplicate",
            Self::StartedRun => "started_run",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for AutomationEvaluationOutcome {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "created_work" => Ok(Self::CreatedWork),
            "skipped_duplicate" => Ok(Self::SkippedDuplicate),
            "started_run" => Ok(Self::StartedRun),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseEnumError("unknown automation evaluation outcome")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AutomationEvaluationView {
    pub id: i64,
    pub project_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: String,
    pub activation_cause: String,
    pub outcome: AutomationEvaluationOutcome,
    pub work_item_id: Option<i64>,
    pub run_id: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemSearchRequest {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default, with = "optional_condition")]
    pub labels: Option<Condition>,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub finished: Option<bool>,
    #[serde(default)]
    pub created_by_run: Option<i64>,
    #[serde(default)]
    pub produced_by_trigger: Option<i64>,
    #[serde(default)]
    pub relationship_kind: Option<String>,
    #[serde(default)]
    pub updated_since: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItemPage {
    pub items: Vec<WorkItemView>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingExplainRequest {
    #[serde(default)]
    pub item_id: Option<i64>,
    #[serde(default)]
    pub rule: Option<AutomationRuleInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoutingExplanationView {
    pub item_id: Option<i64>,
    pub rules: Vec<RoutingRuleExplanationView>,
    pub winner_trigger_id: Option<i64>,
    pub matching_item_count: Option<u64>,
    pub example_items: Vec<WorkItemSummaryView>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoutingRuleExplanationView {
    pub trigger_id: Option<i64>,
    pub trigger_name: String,
    pub selector_matches: bool,
    pub clause_results: Vec<SelectorClauseResultView>,
    pub due: bool,
    pub admission_allowed: bool,
    pub fairness_score: i64,
    pub priority: i64,
    pub exclusive: bool,
    pub suppressed_by_exclusive: bool,
    pub blockers: Vec<String>,
    pub would_win: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SelectorClauseResultView {
    pub path: String,
    pub column_name: Option<String>,
    pub matched: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationRuleInput {
    #[serde(default)]
    pub key: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    #[serde(default = "default_tool_name")]
    pub tool_name: AgentToolName,
    #[serde(default)]
    pub mutability: AutomationRunMutability,
    #[serde(default)]
    pub personality: Option<String>,
    #[serde(default)]
    pub prompt_markdown: String,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub produced_work: Option<ProducedWorkSpec>,
    #[serde(default)]
    pub execution: AutomationExecutionPolicy,
    #[serde(default)]
    pub postconditions: Option<AutomationPostconditions>,
}

fn default_tool_name() -> AgentToolName {
    AgentToolName::Codex
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPersonalityInput {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationBundleManifest {
    pub schema_version: u32,
    pub bundle_key: String,
    pub display_name: String,
    #[serde(default)]
    pub personalities: Vec<AutomationPersonalityInput>,
    #[serde(default)]
    pub automations: Vec<AutomationRuleInput>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleDiffOperation {
    Create,
    Update,
    Delete,
    Unchanged,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleObjectDiffView {
    pub object_type: String,
    pub key: String,
    pub name: String,
    pub operation: BundleDiffOperation,
    pub changes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleDiffView {
    pub bundle_key: String,
    pub display_name: String,
    pub current_hash: Option<String>,
    pub manifest_hash: String,
    pub objects: Vec<BundleObjectDiffView>,
    pub has_deletions: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleYamlRequest {
    pub yaml: String,
    #[serde(default)]
    pub expected_current_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleApplyView {
    pub apply_id: i64,
    pub diff: AutomationBundleDiffView,
    pub status: String,
    pub applied_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct InstalledAutomationBundleView {
    pub apply_id: i64,
    pub bundle_key: String,
    pub display_name: String,
    pub manifest_hash: String,
    pub automation_count: u64,
    pub personality_count: u64,
    pub installed_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveAutomationBundleRequest {
    #[serde(default)]
    pub expected_current_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleValidationView {
    pub manifest: AutomationBundleManifest,
    pub manifest_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleExportView {
    pub yaml: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RestoreRevisionRequest {
    pub revision_id: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct RevisionAnalyticsView {
    pub revision_id: i64,
    pub run_count: u64,
    pub completed_count: u64,
    pub failed_count: u64,
    pub semantic_passed_count: u64,
    pub semantic_failed_count: u64,
    pub total_duration_seconds: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub created_item_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TriggerRunOutcome {
    pub trigger_id: i64,
    pub trigger_name: String,
    pub work_item_id: Option<i64>,
    pub work_item: Option<WorkItemView>,
    pub run: Option<AgentRunView>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessSessionView {
    pub run_id: i64,
    pub project_id: i64,
    pub project_name: String,
    pub tool_name: String,
    pub command: String,
    pub working_dir: String,
    pub process_id: Option<i64>,
    pub output: Vec<AgentRunOutputPiece>,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TriggersPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub settings: Option<ProjectSettingsView>,
    pub personalities: Vec<PersonalityView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AutomationPersonalityInspectorView {
    pub personality: PersonalityView,
    pub revisions: Vec<PersonalityRevisionView>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AutomationRuleInspectorView {
    pub trigger: AutomationTriggerView,
    pub revisions: Vec<AutomationRevisionView>,
    pub evaluations: Vec<AutomationEvaluationView>,
    pub current_revision_analytics: Option<RevisionAnalyticsView>,
}
