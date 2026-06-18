use crate::shared::view_models::{STATE_LABEL_KEY, WorkItemStateView};
use crudkit_leptos::crudkit_core::condition::{
    Condition, ConditionClauseValue, ConditionElement, Operator,
};

const DEFAULT_CREATE_ITEM_STATE: &str = "idea";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CreateItemStateOption {
    pub(crate) identifier: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CreateItemOpenRequest {
    AnyState,
    SingleState(String),
}

pub(crate) fn state_options_for_open_request(
    states: &[WorkItemStateView],
    request: &CreateItemOpenRequest,
) -> Vec<CreateItemStateOption> {
    match request {
        CreateItemOpenRequest::AnyState => state_options_from_project_states(states),
        CreateItemOpenRequest::SingleState(identifier) => states
            .iter()
            .filter(|state| state.identifier == *identifier)
            .map(create_item_state_option)
            .collect(),
    }
}

pub(crate) fn state_options_from_project_states(
    states: &[WorkItemStateView],
) -> Vec<CreateItemStateOption> {
    states.iter().map(create_item_state_option).collect()
}

fn create_item_state_option(state: &WorkItemStateView) -> CreateItemStateOption {
    CreateItemStateOption {
        identifier: state.identifier.clone(),
        name: state.name.clone(),
    }
}

pub(crate) fn default_state_identifier(options: &[CreateItemStateOption]) -> String {
    options
        .iter()
        .find(|option| option.identifier == DEFAULT_CREATE_ITEM_STATE)
        .or_else(|| options.first())
        .map(|option| option.identifier.clone())
        .unwrap_or_else(|| DEFAULT_CREATE_ITEM_STATE.to_owned())
}

pub(crate) fn state_identifier_from_lane_filter(condition: &Condition) -> Option<String> {
    match condition {
        Condition::All(elements) | Condition::Any(elements) => {
            elements.iter().find_map(|element| match element {
                ConditionElement::Clause(clause)
                    if clause.column_name.trim() == STATE_LABEL_KEY
                        && clause.operator == Operator::Equal =>
                {
                    match &clause.value {
                        ConditionClauseValue::String(value) => Some(value.clone()),
                        _ => None,
                    }
                }
                ConditionElement::Clause(_) => None,
                ConditionElement::Condition(condition) => {
                    state_identifier_from_lane_filter(condition)
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crudkit_leptos::crudkit_core::condition::ConditionClause;

    fn state(identifier: &str, name: &str) -> WorkItemStateView {
        WorkItemStateView {
            id: 1,
            project_id: 1,
            identifier: identifier.to_owned(),
            name: name.to_owned(),
            position: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn state_clause(value: ConditionClauseValue) -> ConditionElement {
        ConditionElement::Clause(ConditionClause {
            column_name: format!(" {STATE_LABEL_KEY} "),
            operator: Operator::Equal,
            value,
        })
    }

    #[test]
    fn open_request_for_any_state_returns_all_project_states() {
        let states = [state("open", "Open"), state("done", "Done")];

        let options = state_options_for_open_request(&states, &CreateItemOpenRequest::AnyState);

        assert_eq!(
            options,
            vec![
                CreateItemStateOption {
                    identifier: "open".to_owned(),
                    name: "Open".to_owned(),
                },
                CreateItemStateOption {
                    identifier: "done".to_owned(),
                    name: "Done".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn open_request_for_single_state_requires_a_known_state() {
        let states = [state("open", "Open"), state("done", "Done")];

        let options = state_options_for_open_request(
            &states,
            &CreateItemOpenRequest::SingleState("done".to_owned()),
        );
        let missing = state_options_for_open_request(
            &states,
            &CreateItemOpenRequest::SingleState("missing".to_owned()),
        );

        assert_eq!(
            options,
            vec![CreateItemStateOption {
                identifier: "done".to_owned(),
                name: "Done".to_owned(),
            }]
        );
        assert!(missing.is_empty());
    }

    #[test]
    fn default_state_prefers_idea_then_first_option_then_idea_fallback() {
        let non_idea = vec![
            CreateItemStateOption {
                identifier: "open".to_owned(),
                name: "Open".to_owned(),
            },
            CreateItemStateOption {
                identifier: "done".to_owned(),
                name: "Done".to_owned(),
            },
        ];
        let with_idea = vec![
            CreateItemStateOption {
                identifier: "open".to_owned(),
                name: "Open".to_owned(),
            },
            CreateItemStateOption {
                identifier: "idea".to_owned(),
                name: "Idea".to_owned(),
            },
        ];

        assert_eq!(default_state_identifier(&with_idea), "idea");
        assert_eq!(default_state_identifier(&non_idea), "open");
        assert_eq!(default_state_identifier(&[]), "idea");
    }

    #[test]
    fn lane_state_identifier_comes_from_nested_string_state_equality() {
        let condition = Condition::All(vec![
            ConditionElement::Condition(Box::new(Condition::Any(vec![state_clause(
                ConditionClauseValue::String("open".to_owned()),
            )]))),
            state_clause(ConditionClauseValue::Bool(true)),
        ]);

        assert_eq!(
            state_identifier_from_lane_filter(&condition),
            Some("open".to_owned())
        );
    }

    #[test]
    fn lane_state_identifier_ignores_non_state_or_non_string_clauses() {
        let condition = Condition::All(vec![
            ConditionElement::Clause(ConditionClause {
                column_name: "priority".to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::String("high".to_owned()),
            }),
            state_clause(ConditionClauseValue::Bool(true)),
        ]);

        assert_eq!(state_identifier_from_lane_filter(&condition), None);
    }
}
