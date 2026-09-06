use std::{error::Error, fmt};

#[cfg(feature = "ssr")]
use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
#[cfg(not(feature = "ssr"))]
use crudkit_leptos::crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};

use crate::shared::view_models::WorkItemLabelView;

/// A label-only condition that has been checked before it reaches matching code.
///
/// CrudKit conditions support operators and value types that have no meaning for
/// work item labels. Parsing once gives backend policy and hydrated views the same
/// closed set of supported semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
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
pub(crate) struct LabelConditionError(String);

impl fmt::Display for LabelConditionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LabelConditionError {}

impl ValidatedLabelCondition {
    pub(crate) fn new(condition: &Condition) -> Result<Self, LabelConditionError> {
        Ok(Self {
            condition: LabelCondition::parse(condition)?,
        })
    }

    pub(crate) fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        self.condition.matches(labels)
    }
}

impl LabelCondition {
    fn parse(condition: &Condition) -> Result<Self, LabelConditionError> {
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

fn parse_condition_elements(
    elements: &[ConditionElement],
) -> Result<Vec<LabelConditionElement>, LabelConditionError> {
    elements.iter().map(LabelConditionElement::parse).collect()
}

impl LabelConditionElement {
    fn parse(element: &ConditionElement) -> Result<Self, LabelConditionError> {
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
    fn parse(clause: &ConditionClause) -> Result<Self, LabelConditionError> {
        let key = normalize_key(&clause.column_name)?;
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
                other => Err(LabelConditionError(format!(
                    "label condition '{}' with operator '{}' requires a string, bool, or null value; got {other:?}",
                    clause.column_name,
                    operator_name(clause.operator),
                ))),
            },
            Operator::IsIn => parse_string_list(clause, key),
            operator => Err(LabelConditionError(format!(
                "label condition '{}' uses unsupported operator '{}'",
                clause.column_name,
                operator_name(operator),
            ))),
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

fn parse_string_list(
    clause: &ConditionClause,
    key: String,
) -> Result<LabelClause, LabelConditionError> {
    let ConditionClauseValue::Json(serde_json::Value::Array(values)) = &clause.value else {
        return Err(is_in_value_error(clause));
    };
    let mut expected = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(is_in_value_error(clause));
        };
        expected.push(value.to_owned());
    }
    Ok(LabelClause::ValueIn { key, expected })
}

fn is_in_value_error(clause: &ConditionClause) -> LabelConditionError {
    LabelConditionError(format!(
        "label condition '{}' with is_in requires a JSON array of strings",
        clause.column_name,
    ))
}

fn normalize_key(value: &str) -> Result<String, LabelConditionError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LabelConditionError("label key cannot be empty".to_owned()));
    }
    if value.contains('=') {
        return Err(LabelConditionError(
            "label key cannot contain '='".to_owned(),
        ));
    }
    Ok(value.to_owned())
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
    use crudkit_leptos::crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use serde_json::json;

    use super::*;
    use crate::shared::view_models::{STATE_LABEL_KEY, WorkItemLabelView};

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
    fn conditions_match_absent_labels_and_normalize_keys() {
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
    fn validation_rejects_non_label_operators_and_values() {
        let unsupported_operator =
            Condition::All(vec![ConditionElement::Clause(ConditionClause {
                column_name: STATE_LABEL_KEY.to_owned(),
                operator: Operator::Greater,
                value: ConditionClauseValue::String("open".to_owned()),
            })]);
        let invalid_list = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "priority".to_owned(),
            operator: Operator::IsIn,
            value: ConditionClauseValue::Json(json!(["high", 1])),
        })]);

        assert_that!(
            &(ValidatedLabelCondition::new(&unsupported_operator)
                .unwrap_err()
                .to_string()
                .contains("unsupported operator"))
        )
        .is_true();
        assert_that!(
            &(ValidatedLabelCondition::new(&invalid_list)
                .unwrap_err()
                .to_string()
                .contains("array of strings"))
        )
        .is_true();
    }
}
