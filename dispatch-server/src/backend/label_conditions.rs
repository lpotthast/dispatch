use crudkit_core::condition::Condition;
use rootcause::Result;

use crate::{backend::workflow_labels, shared::view_models::WorkItemLabelView};

pub(crate) use crate::shared::label_conditions::ValidatedLabelCondition;

pub(crate) fn validate_condition(condition: &Condition) -> Result<()> {
    ValidatedLabelCondition::new(condition)?;
    Ok(())
}

impl ValidatedLabelCondition {
    pub(crate) fn matches_automation_selector(&self, labels: &[WorkItemLabelView]) -> bool {
        !workflow_labels::is_automation_blocked(labels) && self.matches(labels)
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };

    use super::*;
    use crate::shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, STATE_LABEL_KEY, WorkItemLabelView,
    };

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

    fn presence_selector(key: &str) -> ValidatedLabelCondition {
        ValidatedLabelCondition::new(&Condition::All(vec![ConditionElement::Clause(
            ConditionClause {
                column_name: key.to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::Bool(true),
            },
        )]))
        .unwrap()
    }

    #[test]
    fn automation_selectors_exclude_blocked_items() {
        let selector = presence_selector(STATE_LABEL_KEY);
        let labels = vec![label(STATE_LABEL_KEY, None)];
        let blocked_labels = vec![
            label(STATE_LABEL_KEY, None),
            label(AUTOMATION_BLOCKED_LABEL_KEY, None),
        ];

        assert_that!(&(selector.matches_automation_selector(&labels))).is_true();
        assert_that!(&(!selector.matches_automation_selector(&blocked_labels))).is_true();
    }
}
