use rootcause::{Result, prelude::*};

use crate::{
    backend::{items::labels::workflow as workflow_labels, projects, storage::utc_now},
    shared::view_models::{AgentReasoningEffort, WorkItemView},
};

use dispatch_types::UpdateWorkItemRequest;

#[derive(Debug)]
pub(crate) struct WorkItemUpdatePlan {
    field_updates: WorkItemFieldUpdates,
    state: Option<String>,
    expect_version: Option<i64>,
}

#[derive(Debug)]
struct WorkItemFieldUpdates {
    title: Option<String>,
    description: Option<String>,
    agent_model_override: Option<Option<String>>,
    agent_reasoning_effort_override: Option<Option<AgentReasoningEffort>>,
}

#[derive(Debug)]
pub(crate) struct AppliedWorkItemUpdate {
    pub(crate) item: WorkItemView,
    pub(crate) state: Option<String>,
    pub(crate) record_item_updated_event: bool,
    pub(crate) agent_model_override: Option<String>,
    pub(crate) agent_reasoning_effort_override: Option<AgentReasoningEffort>,
}

impl WorkItemUpdatePlan {
    pub(crate) fn new(update: UpdateWorkItemRequest) -> Result<Self> {
        let state = update
            .state
            .map(workflow_labels::normalize_state_value)
            .transpose()
            .context("invalid item state")?;
        let field_updates = WorkItemFieldUpdates {
            title: update.title,
            description: update.description,
            agent_model_override: update.agent_model_override,
            agent_reasoning_effort_override: update.agent_reasoning_effort_override,
        };
        if !field_updates.has_any_update() && state.is_none() {
            bail!("item update requires at least one field");
        }

        Ok(Self {
            field_updates,
            state,
            expect_version: update.expect_version,
        })
    }

    pub(crate) fn expect_version(&self) -> Option<i64> {
        self.expect_version
    }

    pub(crate) fn apply_to(self, existing: WorkItemView) -> Result<AppliedWorkItemUpdate> {
        let state = self.state;
        let record_item_updated_event = self.field_updates.has_any_update();
        let field_update = self.field_updates.apply_to(existing)?;

        Ok(AppliedWorkItemUpdate {
            item: field_update.item,
            state,
            record_item_updated_event,
            agent_model_override: field_update.agent_model_override,
            agent_reasoning_effort_override: field_update.agent_reasoning_effort_override,
        })
    }
}

struct AppliedWorkItemFieldUpdate {
    item: WorkItemView,
    agent_model_override: Option<String>,
    agent_reasoning_effort_override: Option<AgentReasoningEffort>,
}

impl WorkItemFieldUpdates {
    fn has_any_update(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.agent_model_override.is_some()
            || self.agent_reasoning_effort_override.is_some()
    }

    fn has_text_update(&self) -> bool {
        self.title.is_some() || self.description.is_some()
    }

    fn apply_to(self, existing: WorkItemView) -> Result<AppliedWorkItemFieldUpdate> {
        let has_text_update = self.has_text_update();
        let Self {
            title,
            description,
            agent_model_override,
            agent_reasoning_effort_override,
        } = self;

        let title = title.unwrap_or_else(|| existing.title.clone());
        let description = description.unwrap_or_else(|| existing.description.clone());
        if has_text_update {
            validate_item_text(&title, &description)?;
        }

        let next_agent_model_override = match agent_model_override {
            Some(agent_model_override) => projects::normalize_optional(agent_model_override),
            None => projects::normalize_optional(existing.agent_model_override.clone()),
        };
        projects::validate_agent_model_field(
            "agent model override",
            next_agent_model_override.as_deref(),
        )?;
        let next_agent_reasoning_effort_override = match agent_reasoning_effort_override {
            Some(agent_reasoning_effort_override) => agent_reasoning_effort_override,
            None => existing.agent_reasoning_effort_override,
        };

        let mut item = existing;
        item.title = title;
        item.description = description;
        item.agent_model_override = next_agent_model_override.clone();
        item.agent_reasoning_effort_override = next_agent_reasoning_effort_override;
        item.version += 1;
        item.updated_at = utc_now();

        Ok(AppliedWorkItemFieldUpdate {
            item,
            agent_model_override: next_agent_model_override,
            agent_reasoning_effort_override: next_agent_reasoning_effort_override,
        })
    }
}

pub(crate) fn validate_item_text(title: &str, description: &str) -> Result<()> {
    if title.trim().is_empty() {
        bail!("item title cannot be empty");
    }
    if description.trim().is_empty() {
        bail!("item description cannot be empty");
    }
    Ok(())
}

pub(crate) fn check_expected_version(expected: Option<i64>, actual: i64) -> Result<()> {
    if let Some(expected) = expected
        && expected != actual
    {
        bail!("version conflict: expected {expected}, found {actual}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;

    use super::*;

    fn work_item() -> WorkItemView {
        WorkItemView {
            id: 7,
            project_id: 3,
            state: None,
            labels: Vec::new(),
            claim_source: None,
            comment_count: 0,
            work_group: None,
            origin: None,
            title: "Existing title".to_owned(),
            description: "Existing description".to_owned(),
            claimed_by: None,
            claimed_at: None,
            claim_expires_at: None,
            finished_at: None,
            agent_model_override: Some("gpt-5.5".to_owned()),
            agent_reasoning_effort_override: Some(AgentReasoningEffort::Medium),
            version: 4,
            created_at: "2026-06-19T00:00:00Z".to_owned(),
            updated_at: "2026-06-19T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn empty_update_is_rejected_before_applying_to_model() {
        let err = WorkItemUpdatePlan::new(UpdateWorkItemRequest::default()).unwrap_err();

        assert_that!(&(err.to_string().contains("requires at least one field"))).is_true();
    }

    #[test]
    fn state_update_is_normalized_without_recording_item_field_event() {
        let plan = WorkItemUpdatePlan::new(UpdateWorkItemRequest {
            state: Some(" review ".to_owned()),
            expect_version: Some(4),
            ..UpdateWorkItemRequest::default()
        })
        .unwrap();

        assert_that!(&(plan.expect_version())).is_equal_to(Some(4));

        let applied = plan.apply_to(work_item()).unwrap();

        assert_that!(&(applied.state.as_deref())).is_equal_to(Some("review"));
        assert_that!(&(!applied.record_item_updated_event)).is_true();
        assert_that!(&(applied.item.version)).is_equal_to(5);
    }

    #[test]
    fn model_override_clear_counts_as_item_field_update() {
        let plan = WorkItemUpdatePlan::new(UpdateWorkItemRequest {
            agent_model_override: Some(None),
            ..UpdateWorkItemRequest::default()
        })
        .unwrap();

        let applied = plan.apply_to(work_item()).unwrap();

        assert_that!(&(applied.record_item_updated_event)).is_true();
        assert_that!(&(applied.item.agent_model_override)).is_equal_to(None);
        assert_that!(&(applied.item.version)).is_equal_to(5);
    }

    #[test]
    fn unknown_model_override_is_rejected() {
        let plan = WorkItemUpdatePlan::new(UpdateWorkItemRequest {
            agent_model_override: Some(Some("gpt-4.1-codex".to_owned())),
            ..UpdateWorkItemRequest::default()
        })
        .unwrap();

        let err = plan.apply_to(work_item()).unwrap_err();

        assert_that!(
            &(err
                .to_string()
                .contains("agent model override must be one of"))
        )
        .is_true();
    }

    #[test]
    fn text_update_validates_effective_title_and_description() {
        let plan = WorkItemUpdatePlan::new(UpdateWorkItemRequest {
            title: Some("  ".to_owned()),
            ..UpdateWorkItemRequest::default()
        })
        .unwrap();

        let err = plan.apply_to(work_item()).unwrap_err();

        assert_that!(&(err.to_string().contains("item title cannot be empty"))).is_true();
    }
}
