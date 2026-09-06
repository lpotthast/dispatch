//! Typed JSON contracts shared by the Dispatch server, API client, CLI, and hydrated frontend.
//!
//! Persistence-specific strings are converted at the server boundary. Enums in this crate retain
//! the canonical JSON and SQLite spelling so callers can use exhaustive Rust matches without
//! changing the wire protocol.

#![cfg_attr(test, recursion_limit = "256")]

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

pub mod knowledge;

mod agents;
mod automation;
mod metrics;
mod projects;
mod requests;
mod runs;
mod work_items;

pub use agents::*;
pub use automation::*;
pub use metrics::*;
pub use projects::*;
pub use requests::*;
pub use runs::*;
pub use work_items::*;

mod optional_condition {
    use crudkit_core::condition::Condition;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    pub(super) fn serialize<S>(
        condition: &Option<Condition>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match condition {
            Some(condition) => serde_json::to_value(condition)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<Condition>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<serde_json::Value>::deserialize(deserializer)?
            .map(|value| serde_json::from_value(value).map_err(D::Error::custom))
            .transpose()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiEvent {
    ProjectListChanged {
        sequence: u64,
        timestamp: String,
    },
    ProjectChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    ProjectDeleted {
        sequence: u64,
        timestamp: String,
        project_id: i64,
        project: String,
    },
    SystemPromptChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    WorkItemChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        item_id: i64,
    },
    CommentChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        item_id: i64,
    },
    SwimLaneChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    WorkItemStateChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    LabelKeyChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        key: String,
    },
    AgentToolChanged {
        sequence: u64,
        timestamp: String,
    },
    AutomationChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    AgentRunChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        run_id: i64,
        item_id: Option<i64>,
    },
    AgentOutputChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        run_id: i64,
        item_id: Option<i64>,
    },
    CodexStatusChanged {
        sequence: u64,
        timestamp: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParseEnumError(&'static str);

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for ParseEnumError {}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn automation_run_mutability_parses_displays_and_serializes() {
        assert_that!(&("mutating".parse::<AutomationRunMutability>().unwrap()))
            .is_equal_to(AutomationRunMutability::Mutating);
        assert_that!(&("read-only".parse::<AutomationRunMutability>().unwrap()))
            .is_equal_to(AutomationRunMutability::ReadOnly);
        assert_that!(&(AutomationRunMutability::ReadOnly.to_string())).is_equal_to("read_only");
        assert_that!(&(serde_json::to_string(&AutomationRunMutability::ReadOnly).unwrap()))
            .is_equal_to(r#""read_only""#);
        assert_that!(&("readonly-ish".parse::<AutomationRunMutability>().is_err())).is_true();
    }

    #[test]
    fn swim_lane_item_order_has_one_canonical_wire_value() {
        assert_that!(&("title-desc".parse::<SwimLaneItemOrder>().unwrap()))
            .is_equal_to(SwimLaneItemOrder::TitleDesc);
        assert_that!(&(serde_json::to_string(&SwimLaneItemOrder::UpdatedDesc).unwrap()))
            .is_equal_to(r#""updated_desc""#);
        assert_that!(&("newest-ish".parse::<SwimLaneItemOrder>().is_err())).is_true();
    }

    #[test]
    fn work_item_event_type_preserves_historical_storage_names() {
        for (storage, event_type) in [
            (
                "SystemPromptChanged",
                WorkItemEventType::SystemPromptChanged,
            ),
            ("MemoryChanged", WorkItemEventType::MemoryChanged),
            ("item_claimed", WorkItemEventType::ItemClaimed),
            (
                "relationship_deleted",
                WorkItemEventType::RelationshipDeleted,
            ),
        ] {
            assert_that!(&(storage.parse::<WorkItemEventType>().unwrap())).is_equal_to(event_type);
            assert_that!(&(event_type.as_storage())).is_equal_to(storage);
            assert_that!(&(serde_json::to_string(&event_type).unwrap()))
                .is_equal_to(format!(r#""{storage}""#));
        }
        assert_that!(&("item_claimedd".parse::<WorkItemEventType>().is_err())).is_true();
    }

    #[test]
    fn agent_run_cleanup_status_rejects_unknown_states() {
        assert_that!(&("not-applicable".parse::<AgentRunCleanupStatus>().unwrap()))
            .is_equal_to(AgentRunCleanupStatus::NotApplicable);
        assert_that!(&(AgentRunCleanupStatus::Cleaned.to_string())).is_equal_to("cleaned");
        assert_that!(&("cleanup_failed".parse::<AgentRunCleanupStatus>().is_err())).is_true();
    }

    #[test]
    fn codex_agent_models_include_gpt_56_matrix() {
        assert_that!(&(CodexAgentModel::newest().as_storage())).is_equal_to("gpt-5.6-sol");
        assert_that!(&("gpt-5.6-terra".parse::<CodexAgentModel>().unwrap()))
            .is_equal_to(CodexAgentModel::Gpt56Terra);
        assert_that!(&("gpt-5.6".parse::<CodexAgentModel>().unwrap()))
            .is_equal_to(CodexAgentModel::Gpt56);
        assert_that!(
            &(CodexAgentModel::Gpt56Sol.supports_reasoning_effort(AgentReasoningEffort::Max))
        )
        .is_true();
        assert_that!(
            &(!CodexAgentModel::Gpt56Sol.supports_reasoning_effort(AgentReasoningEffort::Minimal))
        )
        .is_true();
        assert_that!(
            &(CodexAgentModel::Gpt55.supports_reasoning_effort(AgentReasoningEffort::Minimal))
        )
        .is_true();
        assert_that!(
            &(!CodexAgentModel::Gpt55.supports_reasoning_effort(AgentReasoningEffort::Max))
        )
        .is_true();
    }

    #[test]
    fn agent_reasoning_effort_accepts_max() {
        assert_that!(&("max".parse::<AgentReasoningEffort>().unwrap()))
            .is_equal_to(AgentReasoningEffort::Max);
        assert_that!(&(AgentReasoningEffort::highest())).is_equal_to(AgentReasoningEffort::Max);
        assert_that!(
            &(AgentReasoningEffort::allowed_values()
                .contains("none, minimal, low, medium, high, xhigh, max"))
        )
        .is_true();
    }

    #[test]
    fn create_work_item_request_defaults_missing_initial_labels() {
        let request: CreateWorkItemRequest = serde_json::from_value(serde_json::json!({
            "title": "Backwards compatible",
            "description": "Older callers do not send labels",
            "state": "open",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null
        }))
        .unwrap();

        assert_that!(&(request.initial_labels.is_empty())).is_true();
    }

    #[test]
    fn create_work_item_request_accepts_labels_alias() {
        let request: CreateWorkItemRequest = serde_json::from_value(serde_json::json!({
            "title": "Alias",
            "description": "Accepts labels as an alias",
            "state": "open",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null,
            "labels": [
                { "key": "type", "value": "feature" },
                { "key": "needs-verification", "value": null }
            ]
        }))
        .unwrap();

        assert_that!(&(request.initial_labels.len())).is_equal_to(2);
        assert_that!(&(request.initial_labels[0].key)).is_equal_to("type");
        assert_that!(&(request.initial_labels[0].value.as_deref())).is_equal_to(Some("feature"));
        assert_that!(&(request.initial_labels[1].key)).is_equal_to("needs-verification");
        assert_that!(&(request.initial_labels[1].value.is_none())).is_true();
    }

    #[test]
    fn legacy_agent_runs_default_new_source_view_fields() {
        let run: AgentRunView = serde_json::from_value(serde_json::json!({
            "id": 1,
            "project_id": 2,
            "work_item_id": 3,
            "run_kind": "task",
            "knowledge_revision": null,
            "source_baseline_id": null,
            "trigger_id": null,
            "trigger_name": null,
            "trigger_revision_id": null,
            "personality_revision_id": null,
            "system_prompt_event_id": null,
            "tool_name": "codex",
            "mutability": "mutating",
            "status": "completed",
            "command": "codex",
            "working_dir": "/tmp/project",
            "worktree_path": null,
            "branch_name": null,
            "process_id": null,
            "exit_code": 0,
            "log_path": null,
            "developer_instructions_path": null,
            "user_prompt_path": null,
            "agent_model": null,
            "agent_reasoning_effort": null,
            "effective_input_sha256": null,
            "effective_timeout_seconds": null,
            "effective_concurrency_group": null,
            "token_usage": null,
            "commit_required": false,
            "commit_outcome": "not_required",
            "commit_shas": [],
            "pr_requested": false,
            "pr_url": null,
            "cleanup_status": "not_applicable",
            "worktree_cleaned_at": null,
            "result_summary": "done",
            "semantic_postcondition_status": "not_configured",
            "semantic_postcondition_failures": [],
            "started_at": null,
            "finished_at": null,
            "created_at": "2026-09-04T20:00:00Z",
            "updated_at": "2026-09-04T20:00:00Z"
        }))
        .unwrap();

        assert_that!(&run.source_snapshot_id).is_none();
        assert_that!(&run.launch_target).is_none();
        assert_that!(&run.launch_resolution).is_none();
        assert_that!(&run.knowledge_view_sha256).is_none();
        assert_that!(&run.input_overlay_sha256).is_none();
        assert_that!(&run.source_authority_kind).is_none();
    }
}
