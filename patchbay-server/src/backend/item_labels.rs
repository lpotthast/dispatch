use std::collections::BTreeSet;

use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use rootcause::{Result, prelude::*};

use crate::shared::view_models::{
    AUTOMATION_BLOCKED_LABEL_KEY, CLAIMED_FROM_STATE_LABEL_KEY, CLAIMED_STATE_LABEL,
    DEFAULT_STATE_LABEL, FEEDBACK_REQUESTED_LABEL_KEY, FINISHED_STATE_LABEL, STATE_LABEL_KEY,
    WorkItemLabelView,
};

pub(crate) struct ValidatedLabelCondition<'a> {
    condition: &'a Condition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NormalizedLabel {
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClaimReturnLabelDisposition {
    ClaimableRelease,
    BlockedRelease,
    FeedbackRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkflowLabelPlan<'a> {
    pub(crate) upserts: Vec<WorkflowLabelUpsert<'a>>,
    pub(crate) delete_keys: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkflowLabelUpsert<'a> {
    pub(crate) key: &'static str,
    pub(crate) value: Option<&'a str>,
}

const NEW_CLAIM_DELETES_WORKFLOW_LABELS: &[&str] = &[FEEDBACK_REQUESTED_LABEL_KEY];
const FINISH_DELETES_WORKFLOW_LABELS: &[&str] = &[
    CLAIMED_FROM_STATE_LABEL_KEY,
    AUTOMATION_BLOCKED_LABEL_KEY,
    FEEDBACK_REQUESTED_LABEL_KEY,
];
const CLAIMABLE_RELEASE_DELETES_WORKFLOW_LABELS: &[&str] = FINISH_DELETES_WORKFLOW_LABELS;
const BLOCKED_RELEASE_DELETES_WORKFLOW_LABELS: &[&str] =
    &[CLAIMED_FROM_STATE_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY];
const FEEDBACK_REQUEST_DELETES_WORKFLOW_LABELS: &[&str] = &[CLAIMED_FROM_STATE_LABEL_KEY];

impl<'a> ValidatedLabelCondition<'a> {
    pub(crate) fn new(condition: &'a Condition) -> Result<Self> {
        validate_condition(condition)?;
        Ok(Self { condition })
    }

    pub(crate) fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        condition_matches_validated(self.condition, labels)
    }

    pub(crate) fn matches_automation_selector(&self, labels: &[WorkItemLabelView]) -> bool {
        !is_automation_blocked(labels) && self.matches(labels)
    }
}

pub(crate) fn is_automation_blocked(labels: &[WorkItemLabelView]) -> bool {
    labels.iter().any(|label| {
        label.key == AUTOMATION_BLOCKED_LABEL_KEY || label.key == FEEDBACK_REQUESTED_LABEL_KEY
    })
}

pub(crate) fn source_state_for_new_claim(labels: &[WorkItemLabelView]) -> String {
    current_state(labels).unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned())
}

pub(crate) fn release_state_from_claim_labels(labels: &[WorkItemLabelView]) -> String {
    labels
        .iter()
        .find(|label| label.key == CLAIMED_FROM_STATE_LABEL_KEY)
        .and_then(|label| label.value.clone())
        .or_else(|| current_state(labels))
        .unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned())
}

pub(crate) fn current_state(labels: &[WorkItemLabelView]) -> Option<String> {
    labels
        .iter()
        .find(|label| label.key == STATE_LABEL_KEY)
        .and_then(|label| label.value.clone())
}

pub(crate) fn new_claim_workflow_label_plan(source_state: &str) -> WorkflowLabelPlan<'_> {
    WorkflowLabelPlan {
        upserts: vec![
            WorkflowLabelUpsert {
                key: CLAIMED_FROM_STATE_LABEL_KEY,
                value: Some(source_state),
            },
            WorkflowLabelUpsert {
                key: STATE_LABEL_KEY,
                value: Some(CLAIMED_STATE_LABEL),
            },
        ],
        delete_keys: NEW_CLAIM_DELETES_WORKFLOW_LABELS,
    }
}

pub(crate) fn finish_workflow_label_plan() -> WorkflowLabelPlan<'static> {
    WorkflowLabelPlan {
        upserts: vec![WorkflowLabelUpsert {
            key: STATE_LABEL_KEY,
            value: Some(FINISHED_STATE_LABEL),
        }],
        delete_keys: FINISH_DELETES_WORKFLOW_LABELS,
    }
}

pub(crate) fn claim_return_workflow_label_plan(
    release_state: &str,
    disposition: ClaimReturnLabelDisposition,
) -> WorkflowLabelPlan<'_> {
    let delete_keys = match disposition {
        ClaimReturnLabelDisposition::ClaimableRelease => CLAIMABLE_RELEASE_DELETES_WORKFLOW_LABELS,
        ClaimReturnLabelDisposition::BlockedRelease => BLOCKED_RELEASE_DELETES_WORKFLOW_LABELS,
        ClaimReturnLabelDisposition::FeedbackRequest => FEEDBACK_REQUEST_DELETES_WORKFLOW_LABELS,
    };
    let mut upserts = vec![WorkflowLabelUpsert {
        key: STATE_LABEL_KEY,
        value: Some(release_state),
    }];
    match disposition {
        ClaimReturnLabelDisposition::ClaimableRelease => {}
        ClaimReturnLabelDisposition::BlockedRelease => {
            upserts.push(WorkflowLabelUpsert {
                key: AUTOMATION_BLOCKED_LABEL_KEY,
                value: None,
            });
        }
        ClaimReturnLabelDisposition::FeedbackRequest => {
            upserts.push(WorkflowLabelUpsert {
                key: AUTOMATION_BLOCKED_LABEL_KEY,
                value: None,
            });
            upserts.push(WorkflowLabelUpsert {
                key: FEEDBACK_REQUESTED_LABEL_KEY,
                value: None,
            });
        }
    }

    WorkflowLabelPlan {
        upserts,
        delete_keys,
    }
}

pub(crate) fn normalize_state_value(value: impl Into<String>) -> Result<String> {
    let value = value.into().trim().to_owned();
    if value.is_empty() {
        bail!("state label value cannot be empty");
    }
    if value.contains('=') {
        bail!("state label value cannot contain '='");
    }
    Ok(value)
}

pub(crate) fn normalize_key(value: impl Into<String>) -> Result<String> {
    let value = value.into().trim().to_owned();
    if value.is_empty() {
        bail!("label key cannot be empty");
    }
    if value.contains('=') {
        bail!("label key cannot contain '='");
    }
    Ok(value)
}

pub(crate) fn normalize_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

pub(crate) fn validate_pair(key: &str, value: Option<&str>) -> Result<()> {
    if key == STATE_LABEL_KEY && value.is_none() {
        bail!("state label requires a value");
    }
    Ok(())
}

pub(crate) fn normalize_initial_labels<I>(labels: I) -> Result<Vec<NormalizedLabel>>
where
    I: IntoIterator<Item = (String, Option<String>)>,
{
    let mut normalized = Vec::new();
    let mut keys = BTreeSet::new();
    for (key, value) in labels {
        let key = normalize_key(key)?;
        let value = normalize_value(value);
        validate_pair(&key, value.as_deref())?;
        if key == STATE_LABEL_KEY {
            bail!("initial labels cannot include 'state'; use the state selector");
        }
        if !keys.insert(key.clone()) {
            bail!("duplicate initial label key '{key}'");
        }
        normalized.push(NormalizedLabel { key, value });
    }
    Ok(normalized)
}

pub(crate) fn validate_condition(condition: &Condition) -> Result<()> {
    match condition {
        Condition::All(elements) | Condition::Any(elements) => {
            for element in elements {
                match element {
                    ConditionElement::Clause(clause) => validate_clause(clause)?,
                    ConditionElement::Condition(condition) => validate_condition(condition)?,
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn format_label(key: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{key}={value}"),
        None => key.to_owned(),
    }
}

fn condition_matches_validated(condition: &Condition, labels: &[WorkItemLabelView]) -> bool {
    match condition {
        Condition::All(elements) => {
            for element in elements {
                if !element_matches_validated(element, labels) {
                    return false;
                }
            }
            true
        }
        Condition::Any(elements) => {
            for element in elements {
                if element_matches_validated(element, labels) {
                    return true;
                }
            }
            false
        }
    }
}

fn validate_clause(clause: &ConditionClause) -> Result<()> {
    normalize_key(clause.column_name.clone())?;
    match clause.operator {
        Operator::Equal | Operator::NotEqual => match &clause.value {
            ConditionClauseValue::Bool(_)
            | ConditionClauseValue::String(_)
            | ConditionClauseValue::Json(serde_json::Value::Null) => Ok(()),
            other => bail!(
                "label condition '{}' with operator '{}' requires a string, bool, or null value; got {other:?}",
                clause.column_name,
                operator_name(clause.operator)
            ),
        },
        Operator::IsIn => match &clause.value {
            ConditionClauseValue::Json(serde_json::Value::Array(values))
                if values.iter().all(|value| value.as_str().is_some()) =>
            {
                Ok(())
            }
            _ => bail!(
                "label condition '{}' with is_in requires a JSON array of strings",
                clause.column_name
            ),
        },
        operator => bail!(
            "label condition '{}' uses unsupported operator '{}'",
            clause.column_name,
            operator_name(operator)
        ),
    }
}

fn element_matches_validated(element: &ConditionElement, labels: &[WorkItemLabelView]) -> bool {
    match element {
        ConditionElement::Clause(clause) => clause_matches_validated(clause, labels),
        ConditionElement::Condition(condition) => condition_matches_validated(condition, labels),
    }
}

fn clause_matches_validated(clause: &ConditionClause, labels: &[WorkItemLabelView]) -> bool {
    let key = clause.column_name.trim();
    let label = labels.iter().find(|label| label.key == key);
    let label_value = label.and_then(|label| label.value.as_deref());

    match (&clause.operator, &clause.value) {
        (Operator::Equal, ConditionClauseValue::Bool(expected)) => label.is_some() == *expected,
        (Operator::NotEqual, ConditionClauseValue::Bool(expected)) => label.is_some() != *expected,
        (Operator::Equal, ConditionClauseValue::String(expected)) => {
            label_value == Some(expected.as_str())
        }
        (Operator::NotEqual, ConditionClauseValue::String(expected)) => {
            label_value != Some(expected.as_str())
        }
        (Operator::Equal, ConditionClauseValue::Json(serde_json::Value::Null)) => {
            label.is_some() && label_value.is_none()
        }
        (Operator::NotEqual, ConditionClauseValue::Json(serde_json::Value::Null)) => {
            label.is_none() || label_value.is_some()
        }
        (Operator::IsIn, ConditionClauseValue::Json(serde_json::Value::Array(values))) => {
            let Some(label_value) = label_value else {
                return false;
            };
            values
                .iter()
                .filter_map(|value| value.as_str())
                .any(|expected| expected == label_value)
        }
        _ => false,
    }
}

fn operator_name(operator: Operator) -> &'static str {
    match operator {
        Operator::Equal => "=",
        Operator::NotEqual => "!=",
        Operator::Less => "<",
        Operator::LessOrEqual => "<=",
        Operator::Greater => ">",
        Operator::GreaterOrEqual => ">=",
        Operator::IsIn => "is_in",
    }
}

#[cfg(test)]
mod tests {
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use serde_json::json;

    use super::*;

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
    fn conditions_match_nested_label_presence_and_values() {
        let labels = vec![
            label(STATE_LABEL_KEY, Some("open")),
            label("severity", Some("high")),
            label("bug", None),
        ];
        let selector = Condition::All(vec![
            ConditionElement::Clause(ConditionClause {
                column_name: STATE_LABEL_KEY.to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::String("open".to_owned()),
            }),
            ConditionElement::Condition(Box::new(Condition::Any(vec![
                ConditionElement::Clause(ConditionClause {
                    column_name: "severity".to_owned(),
                    operator: Operator::IsIn,
                    value: ConditionClauseValue::Json(json!(["critical", "high"])),
                }),
                ConditionElement::Clause(ConditionClause {
                    column_name: "bug".to_owned(),
                    operator: Operator::Equal,
                    value: ConditionClauseValue::Bool(true),
                }),
            ]))),
        ]);

        assert!(
            ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches(&labels)
        );
    }

    #[test]
    fn conditions_can_match_absent_labels() {
        let labels = vec![label(STATE_LABEL_KEY, Some("open"))];
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "needs-verification".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        })]);

        assert!(
            ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches(&labels)
        );
    }

    #[test]
    fn validated_label_conditions_match_labels_and_automation_blocking() {
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("open".to_owned()),
        })]);
        let selector = ValidatedLabelCondition::new(&selector).unwrap();
        let labels = vec![label(STATE_LABEL_KEY, Some("open"))];
        let blocked_labels = vec![
            label(STATE_LABEL_KEY, Some("open")),
            label(AUTOMATION_BLOCKED_LABEL_KEY, None),
        ];

        assert!(selector.matches(&labels));
        assert!(selector.matches(&blocked_labels));
        assert!(selector.matches_automation_selector(&labels));
        assert!(!selector.matches_automation_selector(&blocked_labels));
    }

    #[test]
    fn conditions_reject_non_label_operators() {
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Greater,
            value: ConditionClauseValue::String("open".to_owned()),
        })]);

        let err = validate_condition(&selector).unwrap_err();

        assert!(err.to_string().contains("unsupported operator"));
    }

    #[test]
    fn feedback_requested_blocks_automation_claims() {
        let labels = vec![label(FEEDBACK_REQUESTED_LABEL_KEY, None)];

        assert!(is_automation_blocked(&labels));
    }

    #[test]
    fn automation_selector_excludes_blocked_items_even_when_condition_matches() {
        let labels = vec![
            label(STATE_LABEL_KEY, Some("open")),
            label(AUTOMATION_BLOCKED_LABEL_KEY, None),
        ];
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("open".to_owned()),
        })]);

        assert!(
            !ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches_automation_selector(&labels)
        );
    }

    #[test]
    fn release_state_prefers_claim_source_then_current_state_then_default() {
        let labels = vec![
            label(STATE_LABEL_KEY, Some("in_progress")),
            label(CLAIMED_FROM_STATE_LABEL_KEY, Some("review")),
        ];
        assert_eq!(release_state_from_claim_labels(&labels), "review");

        let labels = vec![label(STATE_LABEL_KEY, Some("triage"))];
        assert_eq!(release_state_from_claim_labels(&labels), "triage");

        assert_eq!(release_state_from_claim_labels(&[]), DEFAULT_STATE_LABEL);
    }

    #[test]
    fn new_claim_label_plan_records_source_state_and_clears_feedback_wait() {
        let plan = new_claim_workflow_label_plan("review");

        assert_eq!(
            plan.upserts,
            vec![
                WorkflowLabelUpsert {
                    key: CLAIMED_FROM_STATE_LABEL_KEY,
                    value: Some("review"),
                },
                WorkflowLabelUpsert {
                    key: STATE_LABEL_KEY,
                    value: Some(CLAIMED_STATE_LABEL),
                },
            ]
        );
        assert_eq!(plan.delete_keys, [FEEDBACK_REQUESTED_LABEL_KEY]);
    }

    #[test]
    fn finish_label_plan_closes_item_and_clears_workflow_bookkeeping() {
        let plan = finish_workflow_label_plan();

        assert_eq!(
            plan.upserts,
            vec![WorkflowLabelUpsert {
                key: STATE_LABEL_KEY,
                value: Some(FINISHED_STATE_LABEL),
            }]
        );
        assert_eq!(
            plan.delete_keys,
            [
                CLAIMED_FROM_STATE_LABEL_KEY,
                AUTOMATION_BLOCKED_LABEL_KEY,
                FEEDBACK_REQUESTED_LABEL_KEY,
            ]
        );
    }

    #[test]
    fn claim_return_label_plans_capture_release_feedback_and_retry_policy() {
        let claimable =
            claim_return_workflow_label_plan("open", ClaimReturnLabelDisposition::ClaimableRelease);
        assert_eq!(
            claimable.upserts,
            vec![WorkflowLabelUpsert {
                key: STATE_LABEL_KEY,
                value: Some("open"),
            }]
        );
        assert_eq!(
            claimable.delete_keys,
            [
                CLAIMED_FROM_STATE_LABEL_KEY,
                AUTOMATION_BLOCKED_LABEL_KEY,
                FEEDBACK_REQUESTED_LABEL_KEY,
            ]
        );

        let blocked =
            claim_return_workflow_label_plan("ready", ClaimReturnLabelDisposition::BlockedRelease);
        assert_eq!(
            blocked.upserts,
            vec![
                WorkflowLabelUpsert {
                    key: STATE_LABEL_KEY,
                    value: Some("ready"),
                },
                WorkflowLabelUpsert {
                    key: AUTOMATION_BLOCKED_LABEL_KEY,
                    value: None,
                },
            ]
        );
        assert_eq!(
            blocked.delete_keys,
            [CLAIMED_FROM_STATE_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY]
        );

        let feedback = claim_return_workflow_label_plan(
            "triage",
            ClaimReturnLabelDisposition::FeedbackRequest,
        );
        assert_eq!(
            feedback.upserts,
            vec![
                WorkflowLabelUpsert {
                    key: STATE_LABEL_KEY,
                    value: Some("triage"),
                },
                WorkflowLabelUpsert {
                    key: AUTOMATION_BLOCKED_LABEL_KEY,
                    value: None,
                },
                WorkflowLabelUpsert {
                    key: FEEDBACK_REQUESTED_LABEL_KEY,
                    value: None,
                },
            ]
        );
        assert_eq!(feedback.delete_keys, [CLAIMED_FROM_STATE_LABEL_KEY]);
    }

    #[test]
    fn normalization_rejects_empty_or_composite_keys() {
        assert_eq!(normalize_key(" priority ").unwrap(), "priority");
        assert!(normalize_key("severity=high").is_err());
        assert!(normalize_state_value(" ").is_err());
        assert!(validate_pair(STATE_LABEL_KEY, None).is_err());
    }
}
