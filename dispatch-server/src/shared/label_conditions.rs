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
pub(crate) enum LabelCondition {
    All(Vec<LabelCondition>),
    Any(Vec<LabelCondition>),
    Clause(LabelClause),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LabelClause {
    pub(crate) key: String,
    pub(crate) predicate: LabelPredicate,
    pub(crate) negated: bool,
}

/// Tests on an existing label. Negation belongs to the complete existence check,
/// so an absent label satisfies a negated predicate in every evaluator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LabelPredicate {
    Present,
    ValueEquals(String),
    ValueIsNull,
    ValueIn(Vec<String>),
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

    #[cfg(feature = "ssr")]
    pub(crate) fn condition(&self) -> &LabelCondition {
        &self.condition
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
            Self::Clause(clause) => clause.matches(labels),
        }
    }
}

fn parse_condition_elements(
    elements: &[ConditionElement],
) -> Result<Vec<LabelCondition>, LabelConditionError> {
    elements
        .iter()
        .map(|element| match element {
            ConditionElement::Clause(clause) => {
                Ok(LabelCondition::Clause(LabelClause::parse(clause)?))
            }
            ConditionElement::Condition(condition) => LabelCondition::parse(condition),
        })
        .collect()
}

impl LabelClause {
    fn parse(clause: &ConditionClause) -> Result<Self, LabelConditionError> {
        let key = normalize_key(&clause.column_name)?;
        let mut negated = clause.operator == Operator::NotEqual;
        let predicate = match clause.operator {
            Operator::Equal | Operator::NotEqual => match &clause.value {
                ConditionClauseValue::Bool(expected) => {
                    negated ^= !*expected;
                    Ok(LabelPredicate::Present)
                }
                ConditionClauseValue::String(expected) => {
                    Ok(LabelPredicate::ValueEquals(expected.clone()))
                }
                ConditionClauseValue::Json(serde_json::Value::Null) => {
                    Ok(LabelPredicate::ValueIsNull)
                }
                other => Err(LabelConditionError(format!(
                    "label condition '{}' with operator '{}' requires a string, bool, or null value; got {other:?}",
                    clause.column_name,
                    operator_name(clause.operator),
                ))),
            },
            Operator::IsIn => parse_string_list(clause).map(LabelPredicate::ValueIn),
            operator => Err(LabelConditionError(format!(
                "label condition '{}' uses unsupported operator '{}'",
                clause.column_name,
                operator_name(operator),
            ))),
        }?;
        Ok(Self {
            key,
            predicate,
            negated,
        })
    }

    fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        let matches = labels
            .iter()
            .find(|label| label.key == self.key)
            .is_some_and(|label| match &self.predicate {
                LabelPredicate::Present => true,
                LabelPredicate::ValueEquals(expected) => label.value.as_ref() == Some(expected),
                LabelPredicate::ValueIsNull => label.value.is_none(),
                LabelPredicate::ValueIn(expected) => label
                    .value
                    .as_ref()
                    .is_some_and(|value| expected.contains(value)),
            });
        matches != self.negated
    }
}

fn parse_string_list(clause: &ConditionClause) -> Result<Vec<String>, LabelConditionError> {
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
    Ok(expected)
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
    fn clauses_preserve_presence_null_and_negation_truth_tables() {
        let cases = [
            (
                Operator::Equal,
                ConditionClauseValue::Bool(true),
                [false, true, true, true],
            ),
            (
                Operator::Equal,
                ConditionClauseValue::Bool(false),
                [true, false, false, false],
            ),
            (
                Operator::NotEqual,
                ConditionClauseValue::Bool(true),
                [true, false, false, false],
            ),
            (
                Operator::NotEqual,
                ConditionClauseValue::Bool(false),
                [false, true, true, true],
            ),
            (
                Operator::Equal,
                ConditionClauseValue::String("high".to_owned()),
                [false, false, true, false],
            ),
            (
                Operator::NotEqual,
                ConditionClauseValue::String("high".to_owned()),
                [true, true, false, true],
            ),
            (
                Operator::Equal,
                ConditionClauseValue::Json(json!(null)),
                [false, true, false, false],
            ),
            (
                Operator::NotEqual,
                ConditionClauseValue::Json(json!(null)),
                [true, false, true, true],
            ),
            (
                Operator::IsIn,
                ConditionClauseValue::Json(json!(["high", "low"])),
                [false, false, true, true],
            ),
            (
                Operator::IsIn,
                ConditionClauseValue::Json(json!([])),
                [false, false, false, false],
            ),
        ];
        let labels = [
            vec![],
            vec![label("priority", None)],
            vec![label("priority", Some("high"))],
            vec![label("priority", Some("low"))],
        ];
        for (operator, value, expected) in cases {
            let condition =
                ValidatedLabelCondition::new(&Condition::All(vec![ConditionElement::Clause(
                    ConditionClause {
                        column_name: " priority ".to_owned(),
                        operator,
                        value,
                    },
                )]))
                .unwrap();
            assert_that!(&labels.each_ref().map(|labels| condition.matches(labels)))
                .is_equal_to(expected);
        }
        assert_that!(
            &ValidatedLabelCondition::new(&Condition::All(vec![]))
                .unwrap()
                .matches(&[])
        )
        .is_true();
        assert_that!(
            &ValidatedLabelCondition::new(&Condition::Any(vec![]))
                .unwrap()
                .matches(&[])
        )
        .is_false();
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
