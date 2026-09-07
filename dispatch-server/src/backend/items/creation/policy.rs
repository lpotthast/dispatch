use super::model::CreateWorkItem;
use crate::backend::{
    items::labels::policy as item_labels, items::labels::workflow as workflow_labels, projects,
};
use dispatch_types::AgentReasoningEffort;
use rootcause::{Result, prelude::*};
#[derive(Debug)]
pub(crate) struct CreateWorkItemPlan {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) state_label: String,
    pub(crate) agent_model_override: Option<String>,
    pub(crate) agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    pub(crate) initial_labels: Vec<item_labels::NormalizedLabel>,
}

impl CreateWorkItemPlan {
    pub(crate) fn new(create: CreateWorkItem) -> Result<Self> {
        crate::backend::items::policy::validate_item_text(&create.title, &create.description)?;
        let state_label = workflow_labels::normalize_state_value(create.state)?;
        let agent_model_override = projects::normalize_optional(create.agent_model_override);
        projects::validate_agent_model_field(
            "agent model override",
            agent_model_override.as_deref(),
        )?;
        let initial_labels = item_labels::normalize_initial_labels(
            create
                .initial_labels
                .into_iter()
                .map(|label| (label.key, label.value)),
        )
        .context("invalid initial labels")?;

        Ok(Self {
            title: create.title,
            description: create.description,
            state_label,
            agent_model_override,
            agent_reasoning_effort_override: create.agent_reasoning_effort_override,
            initial_labels,
        })
    }

    pub(crate) fn agent_model_override(&self) -> Option<&str> {
        self.agent_model_override.as_deref()
    }

    pub(crate) fn agent_reasoning_effort_override(&self) -> Option<AgentReasoningEffort> {
        self.agent_reasoning_effort_override
    }
}
#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use dispatch_types::CreateWorkItemLabelRequest;

    use super::*;
    use crate::shared::view_models::STATE_LABEL_KEY;

    fn create_work_item() -> CreateWorkItem {
        CreateWorkItem {
            title: "Create me".to_owned(),
            description: "Exercise create planning".to_owned(),
            state: " open ".to_owned(),
            agent_model_override: Some("  ".to_owned()),
            agent_reasoning_effort_override: Some(AgentReasoningEffort::Medium),
            initial_labels: vec![
                CreateWorkItemLabelRequest {
                    key: " priority ".to_owned(),
                    value: Some(" high ".to_owned()),
                },
                CreateWorkItemLabelRequest {
                    key: "needs-verification".to_owned(),
                    value: Some("  ".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn create_plan_normalizes_state_overrides_and_initial_labels() {
        let plan = CreateWorkItemPlan::new(create_work_item()).unwrap();
        let insert = plan;

        assert_that!(&(insert.state_label)).is_equal_to("open");
        assert_that!(&(insert.initial_labels)).is_equal_to(vec![
            item_labels::NormalizedLabel {
                key: "priority".to_owned(),
                value: Some("high".to_owned()),
            },
            item_labels::NormalizedLabel {
                key: "needs-verification".to_owned(),
                value: None,
            },
        ]);

        assert_that!(&insert.title).is_equal_to("Create me");
        assert_that!(&insert.agent_model_override).is_none();
        assert_that!(&insert.agent_reasoning_effort_override)
            .is_equal_to(Some(AgentReasoningEffort::Medium));
    }

    #[test]
    fn create_plan_rejects_invalid_text_state_and_labels() {
        let err = CreateWorkItemPlan::new(CreateWorkItem {
            title: " ".to_owned(),
            ..create_work_item()
        })
        .unwrap_err();
        assert_that!(&(err.to_string().contains("item title cannot be empty"))).is_true();

        let err = CreateWorkItemPlan::new(CreateWorkItem {
            state: " ".to_owned(),
            ..create_work_item()
        })
        .unwrap_err();
        assert_that!(
            &(err
                .to_string()
                .contains("state label value cannot be empty"))
        )
        .is_true();

        let err = CreateWorkItemPlan::new(CreateWorkItem {
            initial_labels: vec![CreateWorkItemLabelRequest {
                key: STATE_LABEL_KEY.to_owned(),
                value: Some("review".to_owned()),
            }],
            ..create_work_item()
        })
        .unwrap_err();
        assert_that!(&(err.to_string().contains("invalid initial labels"))).is_true();
        assert_that!(&(err.to_string().contains("use the state selector"))).is_true();
    }

    #[test]
    fn create_plan_rejects_unknown_model_override() {
        let err = CreateWorkItemPlan::new(CreateWorkItem {
            agent_model_override: Some("gpt-4.1-codex".to_owned()),
            ..create_work_item()
        })
        .unwrap_err();

        assert_that!(
            &(err
                .to_string()
                .contains("agent model override must be one of"))
        )
        .is_true();
    }
}
