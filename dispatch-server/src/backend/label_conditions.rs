use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use rootcause::{Result, prelude::*};

use crate::{
    backend::{item_labels, workflow_labels},
    shared::view_models::WorkItemLabelView,
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
        !workflow_labels::is_automation_blocked(labels) && self.matches(labels)
    }
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
        let key = item_labels::normalize_key(clause.column_name.clone())?;
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
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use serde_json::json;

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

        assert_that!(
            &(ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches(&labels))
        )
        .is_true();
    }

    #[test]
    fn conditions_can_match_absent_labels() {
        let labels = vec![label(STATE_LABEL_KEY, Some("open"))];
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "needs-verification".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        })]);

        assert_that!(
            &(ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches(&labels))
        )
        .is_true();
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

        assert_that!(
            &(ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches(&labels))
        )
        .is_true();
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

        assert_that!(&(selector.matches(&labels))).is_true();
        assert_that!(&(selector.matches(&blocked_labels))).is_true();
        assert_that!(&(selector.matches_automation_selector(&labels))).is_true();
        assert_that!(&(!selector.matches_automation_selector(&blocked_labels))).is_true();
    }

    #[test]
    fn conditions_reject_non_label_operators() {
        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Greater,
            value: ConditionClauseValue::String("open".to_owned()),
        })]);

        let err = validate_condition(&selector).unwrap_err();

        assert_that!(&(err.to_string().contains("unsupported operator"))).is_true();
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

        assert_that!(
            &(!ValidatedLabelCondition::new(&selector)
                .unwrap()
                .matches_automation_selector(&labels))
        )
        .is_true();
    }
}
