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

pub(crate) struct ValidatedLabelCondition {
    condition: LabelCondition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelCondition {
    All(Vec<LabelConditionElement>),
    Any(Vec<LabelConditionElement>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelConditionElement {
    Clause(LabelClause),
    Condition(Box<LabelCondition>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelClause {
    PresenceEquals { key: String, present: bool },
    ValueEquals { key: String, expected: String },
    ValueNotEquals { key: String, expected: String },
    ValueIsNull { key: String },
    ValueIsNotNull { key: String },
    ValueIn { key: String, expected: Vec<String> },
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

impl ValidatedLabelCondition {
    pub(crate) fn new(condition: &Condition) -> Result<Self> {
        Ok(Self {
            condition: LabelCondition::parse(condition)?,
        })
    }

    pub(crate) fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        self.condition.matches(labels)
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
    LabelCondition::parse(condition).map(|_| ())
}

impl LabelCondition {
    fn parse(condition: &Condition) -> Result<Self> {
        match condition {
            Condition::All(elements) => Ok(Self::All(parse_condition_elements(elements)?)),
            Condition::Any(elements) => Ok(Self::Any(parse_condition_elements(elements)?)),
        }
    }

    fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        match self {
            Self::All(elements) => elements.iter().all(|element| element.matches(labels)),
            Self::Any(elements) => elements.iter().any(|element| element.matches(labels)),
        }
    }
}

fn parse_condition_elements(elements: &[ConditionElement]) -> Result<Vec<LabelConditionElement>> {
    elements.iter().map(LabelConditionElement::parse).collect()
}

impl LabelConditionElement {
    fn parse(element: &ConditionElement) -> Result<Self> {
        match element {
            ConditionElement::Clause(clause) => Ok(Self::Clause(LabelClause::parse(clause)?)),
            ConditionElement::Condition(condition) => {
                Ok(Self::Condition(Box::new(LabelCondition::parse(condition)?)))
            }
        }
    }

    fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        match self {
            Self::Clause(clause) => clause.matches(labels),
            Self::Condition(condition) => condition.matches(labels),
        }
    }
}

impl LabelClause {
    fn parse(clause: &ConditionClause) -> Result<Self> {
        let key = normalize_key(clause.column_name.clone())?;
        match clause.operator {
            Operator::Equal | Operator::NotEqual => match &clause.value {
                ConditionClauseValue::Bool(expected) => Ok(Self::PresenceEquals {
                    key,
                    present: if clause.operator == Operator::Equal {
                        *expected
                    } else {
                        !*expected
                    },
                }),
                ConditionClauseValue::String(expected) => {
                    if clause.operator == Operator::Equal {
                        Ok(Self::ValueEquals {
                            key,
                            expected: expected.clone(),
                        })
                    } else {
                        Ok(Self::ValueNotEquals {
                            key,
                            expected: expected.clone(),
                        })
                    }
                }
                ConditionClauseValue::Json(serde_json::Value::Null) => {
                    if clause.operator == Operator::Equal {
                        Ok(Self::ValueIsNull { key })
                    } else {
                        Ok(Self::ValueIsNotNull { key })
                    }
                }
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
                    let expected = values
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .expect("is_in values were validated as strings")
                                .to_owned()
                        })
                        .collect();
                    Ok(Self::ValueIn { key, expected })
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

    fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        match self {
            Self::PresenceEquals { key, present } => find_label(labels, key).is_some() == *present,
            Self::ValueEquals { key, expected } => {
                label_value(labels, key) == Some(expected.as_str())
            }
            Self::ValueNotEquals { key, expected } => {
                label_value(labels, key) != Some(expected.as_str())
            }
            Self::ValueIsNull { key } => find_label(labels, key)
                .map(|label| label.value.is_none())
                .unwrap_or(false),
            Self::ValueIsNotNull { key } => find_label(labels, key)
                .map(|label| label.value.is_some())
                .unwrap_or(true),
            Self::ValueIn { key, expected } => label_value(labels, key)
                .map(|value| expected.iter().any(|expected| expected == value))
                .unwrap_or(false),
        }
    }
}

fn find_label<'a>(labels: &'a [WorkItemLabelView], key: &str) -> Option<&'a WorkItemLabelView> {
    labels.iter().find(|label| label.key == key)
}

fn label_value<'a>(labels: &'a [WorkItemLabelView], key: &str) -> Option<&'a str> {
    find_label(labels, key).and_then(|label| label.value.as_deref())
}

pub(crate) fn format_label(key: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{key}={value}"),
        None => key.to_owned(),
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
    fn parsed_conditions_normalize_keys_and_preserve_negative_matchers() {
        let labels = vec![
            label("priority", Some("high")),
            label("ready", None),
            label("reviewed", None),
        ];
        let selector = Condition::All(vec![
            ConditionElement::Clause(ConditionClause {
                column_name: " priority ".to_owned(),
                operator: Operator::NotEqual,
                value: ConditionClauseValue::String("low".to_owned()),
            }),
            ConditionElement::Clause(ConditionClause {
                column_name: "ready".to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::Json(serde_json::Value::Null),
            }),
            ConditionElement::Clause(ConditionClause {
                column_name: "reviewed".to_owned(),
                operator: Operator::NotEqual,
                value: ConditionClauseValue::Bool(false),
            }),
            ConditionElement::Clause(ConditionClause {
                column_name: "missing".to_owned(),
                operator: Operator::NotEqual,
                value: ConditionClauseValue::Json(serde_json::Value::Null),
            }),
        ]);

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
