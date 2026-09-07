use crate::backend::{
    items::labels::conditions as label_conditions, items::labels::workflow as workflow_labels,
};
use crudkit_core::condition::Condition;
use dispatch_types::{AuthorType, WorkItemEventType, WorkItemLabelView};
use rootcause::Result;
pub(super) enum ClaimSelector {
    State(String),
    AutomationCondition(label_conditions::ValidatedLabelCondition),
}

impl ClaimSelector {
    pub(super) fn state(state: impl Into<String>) -> Result<Self> {
        Ok(Self::State(workflow_labels::normalize_state_value(state)?))
    }

    pub(super) fn automation_condition(condition: &Condition) -> Result<Self> {
        Ok(Self::AutomationCondition(
            label_conditions::ValidatedLabelCondition::new(condition)?,
        ))
    }

    pub(super) fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        match self {
            Self::State(state) => {
                !workflow_labels::is_automation_blocked(labels)
                    && workflow_labels::current_state(labels).as_deref() == Some(state.as_str())
            }
            Self::AutomationCondition(selector) => selector.matches_automation_selector(labels),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReleaseAutomationDisposition {
    Claimable,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutomationClaimOutcome {
    CompletedUnfinished,
    Failed,
    Cancelled,
}

pub(crate) struct AutomationClaimFinalization<'a> {
    pub(crate) project_id: i64,
    pub(crate) project_name: &'a str,
    pub(crate) run_id: i64,
    pub(crate) claimed_item_id: Option<i64>,
    pub(crate) agent_id: &'a str,
    pub(crate) outcome: AutomationClaimOutcome,
    pub(crate) detail: Option<&'a str>,
}

#[derive(Clone)]
pub(super) struct ClaimCandidate {
    pub(super) item_id: i64,
    pub(super) observed_version: i64,
    pub(super) source_state: String,
    pub(super) updated_at: String,
}
pub(super) struct ClaimRecord<'a> {
    pub(super) labels: Option<workflow_labels::WorkflowLabelPlan<'a>>,
    pub(super) comment: Option<(AuthorType, &'a str)>,
    pub(super) event_type: WorkItemEventType,
    pub(super) body: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY, STATE_LABEL_KEY,
    };
    use assertr::prelude::*;

    fn label(key: &str, value: Option<&str>) -> WorkItemLabelView {
        WorkItemLabelView {
            id: 1,
            project_id: 1,
            work_item_id: 1,
            key: key.to_owned(),
            value: value.map(ToOwned::to_owned),
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn state_selector_matches_current_state_and_skips_workflow_blockers() {
        let selector = ClaimSelector::state(" open ").unwrap();

        assert_that!(&(selector.matches(&[label(STATE_LABEL_KEY, Some("open"))]))).is_true();
        assert_that!(&(!selector.matches(&[label(STATE_LABEL_KEY, Some("idea"))]))).is_true();
        assert_that!(
            &(!selector.matches(&[
                label(STATE_LABEL_KEY, Some("open")),
                label(AUTOMATION_BLOCKED_LABEL_KEY, None),
            ]))
        )
        .is_true();
        assert_that!(
            &(!selector.matches(&[
                label(STATE_LABEL_KEY, Some("open")),
                label(FEEDBACK_REQUESTED_LABEL_KEY, None),
            ]))
        )
        .is_true();
    }
}
