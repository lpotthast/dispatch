use std::collections::BTreeSet;

use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    backend::{
        entities::{
            work_item_event,
            work_item_origin::{self, WorkItemOrigin},
        },
        item_labels, items, label_conditions,
        storage::Store,
    },
    shared::view_models::{
        AgentCommitOutcome, AutomationPostconditions, ExpectedDisposition, LabelAssertion,
        LabelAssertionKind, PostconditionFailureView, SemanticPostconditionStatus,
        WorkItemEventType, WorkItemView, WorkspaceAssertion,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct SemanticEvaluation {
    pub(crate) status: SemanticPostconditionStatus,
    pub(crate) failures: Vec<PostconditionFailureView>,
}

pub(crate) async fn evaluate(
    store: &Store,
    project_name: &str,
    run_id: i64,
    baseline_item: Option<&WorkItemView>,
    postconditions: Option<&AutomationPostconditions>,
    commit_outcome: AgentCommitOutcome,
) -> Result<SemanticEvaluation> {
    let Some(postconditions) = postconditions else {
        return Ok(SemanticEvaluation {
            status: SemanticPostconditionStatus::NotConfigured,
            failures: Vec::new(),
        });
    };
    if postconditions.any_of.is_empty() {
        bail!("automation postconditions require at least one outcome set");
    }

    let current_item = match baseline_item {
        Some(item) => Some(items::get_item(store, project_name, item.id).await?),
        None => None,
    };
    let events = work_item_event::Entity::find()
        .filter(work_item_event::Column::AgentRunId.eq(run_id))
        .all(store.db().as_ref())
        .await
        .context("failed to load run-attributed events for postconditions")?;
    let event_types = events
        .iter()
        .map(|event| event.event_type.parse::<WorkItemEventType>())
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let origins = WorkItemOrigin::find()
        .filter(work_item_origin::Column::AgentRunId.eq(run_id))
        .all(store.db().as_ref())
        .await
        .context("failed to load run-attributed created items for postconditions")?;
    let mut created_items = Vec::with_capacity(origins.len());
    for origin in origins {
        created_items.push(items::get_item(store, project_name, origin.work_item_id).await?);
    }
    let workspace_changed = !matches!(
        commit_outcome,
        AgentCommitOutcome::SkippedNoChanges | AgentCommitOutcome::SkippedNoGitRepo
    );

    let mut all_failures = Vec::new();
    for (outcome_index, outcome) in postconditions.any_of.iter().enumerate() {
        let mut failures = Vec::new();
        if let Some(expected) = outcome.disposition {
            let actual = actual_disposition(&event_types);
            if actual != expected {
                failures.push(failure(
                    outcome_index,
                    "disposition",
                    format!("{expected:?}"),
                    format!("{actual:?}"),
                ));
            }
        }
        for expected in &outcome.attributed_events {
            if !event_types.contains(expected) {
                failures.push(failure(
                    outcome_index,
                    "attributed_event",
                    expected.as_storage(),
                    "missing",
                ));
            }
        }
        for assertion in &outcome.labels {
            evaluate_label_assertion(
                outcome_index,
                assertion,
                baseline_item,
                current_item.as_ref(),
                &events,
                &mut failures,
            );
        }
        if let Some(assertion) = &outcome.created_items {
            evaluate_created_item_assertion(
                outcome_index,
                "created_items",
                assertion,
                &created_items,
                &mut failures,
            )?;
        }
        for (assertion_index, assertion) in outcome.created_item_assertions.iter().enumerate() {
            evaluate_created_item_assertion(
                outcome_index,
                &format!("created_item_assertions[{assertion_index}]"),
                assertion,
                &created_items,
                &mut failures,
            )?;
        }
        if outcome.created_items_share_group {
            let group_ids = created_items
                .iter()
                .filter_map(|item| item.work_group.as_ref().map(|group| group.id))
                .collect::<BTreeSet<_>>();
            if created_items.is_empty()
                || group_ids.len() != 1
                || created_items.iter().any(|item| item.work_group.is_none())
            {
                failures.push(failure(
                    outcome_index,
                    "created_items_share_group",
                    "all run-created items assigned to one work group",
                    format!(
                        "created_items={}, assigned_items={}, distinct_groups={}",
                        created_items.len(),
                        created_items
                            .iter()
                            .filter(|item| item.work_group.is_some())
                            .count(),
                        group_ids.len()
                    ),
                ));
            }
        }
        if let Some(assertion) = outcome.workspace_changes {
            let passed = match assertion {
                WorkspaceAssertion::Any => true,
                WorkspaceAssertion::None => !workspace_changed,
                WorkspaceAssertion::Required => workspace_changed,
            };
            if !passed {
                failures.push(failure(
                    outcome_index,
                    "workspace_changes",
                    format!("{assertion:?}"),
                    workspace_changed.to_string(),
                ));
            }
        }
        if failures.is_empty() {
            return Ok(SemanticEvaluation {
                status: SemanticPostconditionStatus::Passed,
                failures: Vec::new(),
            });
        }
        all_failures.extend(failures);
    }

    Ok(SemanticEvaluation {
        status: SemanticPostconditionStatus::Failed,
        failures: all_failures,
    })
}

fn evaluate_created_item_assertion(
    outcome_index: usize,
    assertion_name: &str,
    assertion: &crate::shared::view_models::CreatedItemAssertion,
    created_items: &[WorkItemView],
    failures: &mut Vec<PostconditionFailureView>,
) -> Result<()> {
    let matching = match &assertion.selector {
        Some(selector) => {
            let selector = label_conditions::ValidatedLabelCondition::new(selector)?;
            created_items
                .iter()
                .filter(|item| selector.matches(&item.labels))
                .count() as u64
        }
        None => created_items.len() as u64,
    };
    if let Some(count) = assertion.count
        && matching != count
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.count"),
            count.to_string(),
            matching.to_string(),
        ));
    }
    if let Some(minimum) = assertion.at_least
        && matching < minimum
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.at_least"),
            minimum.to_string(),
            matching.to_string(),
        ));
    }
    if let Some(maximum) = assertion.at_most
        && matching > maximum
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.at_most"),
            maximum.to_string(),
            matching.to_string(),
        ));
    }
    Ok(())
}

fn actual_disposition(events: &BTreeSet<WorkItemEventType>) -> ExpectedDisposition {
    if events.contains(&WorkItemEventType::ItemFinished) {
        ExpectedDisposition::Finished
    } else if events.contains(&WorkItemEventType::FeedbackRequested) {
        ExpectedDisposition::FeedbackRequested
    } else if events.contains(&WorkItemEventType::ItemReleased) {
        ExpectedDisposition::Released
    } else {
        ExpectedDisposition::SuccessfulNonterminal
    }
}

fn evaluate_label_assertion(
    outcome_index: usize,
    assertion: &LabelAssertion,
    baseline: Option<&WorkItemView>,
    current: Option<&WorkItemView>,
    events: &[work_item_event::Model],
    failures: &mut Vec<PostconditionFailureView>,
) {
    let before = baseline.is_some_and(|item| has_label(item, assertion));
    let after = current.is_some_and(|item| has_label(item, assertion));
    let attributed_transition = baseline.is_some_and(|item| {
        events
            .iter()
            .any(|event| label_event_matches(event, item.id, assertion))
    });
    let passed = match assertion.assertion {
        LabelAssertionKind::Added => !before && after && attributed_transition,
        LabelAssertionKind::Removed => before && !after && attributed_transition,
        LabelAssertionKind::Present => after,
        LabelAssertionKind::Absent => !after,
    };
    if !passed {
        failures.push(failure(
            outcome_index,
            format!("label_{:?}", assertion.assertion).to_lowercase(),
            match assertion.value.as_deref() {
                Some(value) => format!("{}={value}", assertion.key),
                None => assertion.key.clone(),
            },
            format!("before={before}, after={after}"),
        ));
    }
}

fn label_event_matches(
    event: &work_item_event::Model,
    item_id: i64,
    assertion: &LabelAssertion,
) -> bool {
    if event.work_item_id != Some(item_id) {
        return false;
    }
    let expected = item_labels::format_label(&assertion.key, assertion.value.as_deref());
    match assertion.assertion {
        LabelAssertionKind::Added => {
            (event.event_type == WorkItemEventType::LabelAdded.as_storage()
                && event.body == format!("Added label {expected}"))
                || (event.event_type == WorkItemEventType::LabelUpdated.as_storage()
                    && event.body == format!("Updated label {expected}"))
        }
        LabelAssertionKind::Removed => {
            (event.event_type == WorkItemEventType::LabelDeleted.as_storage()
                && event.body == format!("Deleted label {expected}"))
                || event.event_type == WorkItemEventType::LabelUpdated.as_storage()
        }
        LabelAssertionKind::Present | LabelAssertionKind::Absent => true,
    }
}

fn has_label(item: &WorkItemView, assertion: &LabelAssertion) -> bool {
    item.labels.iter().any(|label| {
        label.key == assertion.key
            && assertion
                .value
                .as_ref()
                .is_none_or(|value| label.value.as_ref() == Some(value))
    })
}

fn failure(
    outcome_index: usize,
    assertion: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> PostconditionFailureView {
    PostconditionFailureView {
        outcome_index,
        assertion: assertion.into(),
        expected: expected.into(),
        actual: actual.into(),
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use tempfile::TempDir;

    use super::*;
    use crate::{
        backend::{
            entities::{
                agent_run::AgentRunActiveModel,
                work_item_origin::{WorkItemOrigin, WorkItemOriginActiveModel},
            },
            item_label_service,
            items::{CreateWorkItem, create_item},
            projects::{self, CreateProject, create_project},
            request_attribution::RequestAttribution,
            storage::utc_now,
            work_item_events::{self, EventAttribution},
            work_item_groups,
        },
        shared::view_models::{
            AuthorType, AutomationOutcomeSet, CreateWorkItemGroupRequest,
            CreateWorkItemLabelRequest, CreatedItemAssertion, WorkItemOriginKind,
        },
    };

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        (temp, store)
    }

    async fn insert_run(store: &Store) -> i64 {
        let project_id = projects::project_id(store, "demo").await.unwrap();
        let now = utc_now();
        AgentRunActiveModel {
            project_id: Set(project_id),
            tool_name: Set("codex".to_owned()),
            mutability: Set("read_only".to_owned()),
            status: Set("running".to_owned()),
            command: Set(String::new()),
            working_dir: Set(String::new()),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(store.db().as_ref())
        .await
        .unwrap()
        .id
    }

    async fn base_item(store: &Store) -> WorkItemView {
        create_item(
            store,
            "demo",
            CreateWorkItem {
                title: "Postcondition target".to_owned(),
                description: "Evaluate semantic outcomes".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn unconfigured_postconditions_are_observational() {
        let (_temp, store) = test_store().await;
        let run_id = insert_run(&store).await;
        let result = evaluate(
            &store,
            "demo",
            run_id,
            None,
            None,
            AgentCommitOutcome::NotRequired,
        )
        .await
        .unwrap();
        assert_that!(&(result.status)).is_equal_to(SemanticPostconditionStatus::NotConfigured);
        assert_that!(&(result.failures.is_empty())).is_true();
    }

    #[tokio::test]
    async fn alternative_outcome_can_require_attributed_labels_created_items_and_workspace() {
        let (_temp, store) = test_store().await;
        let run_id = insert_run(&store).await;
        let baseline = base_item(&store).await;
        let agent_id = format!("agent-run-{run_id}");
        item_label_service::add_label_with_attribution(
            &store,
            "demo",
            baseline.id,
            "approved".to_owned(),
            None,
            None,
            EventAttribution {
                actor_type: Some(AuthorType::Agent),
                actor_id: Some(&agent_id),
                agent_run_id: Some(run_id),
            },
        )
        .await
        .unwrap();
        let child = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: "Attributed child".to_owned(),
                description: "Created by the run".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: "kind".to_owned(),
                    value: Some("child".to_owned()),
                }],
            },
        )
        .await
        .unwrap();
        let second_child = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: "Second attributed child".to_owned(),
                description: "Also created by the run".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: "kind".to_owned(),
                    value: Some("child".to_owned()),
                }],
            },
        )
        .await
        .unwrap();
        for child_id in [child.id, second_child.id] {
            let origin = WorkItemOrigin::find_by_id(child_id)
                .one(store.db().as_ref())
                .await
                .unwrap()
                .unwrap();
            let mut origin: WorkItemOriginActiveModel = origin.into();
            origin.origin_kind = Set(WorkItemOriginKind::AgentRun.as_storage().to_owned());
            origin.actor_id = Set(Some(agent_id.clone()));
            origin.agent_run_id = Set(Some(run_id));
            origin.update(store.db().as_ref()).await.unwrap();
        }
        let attribution = RequestAttribution::default();
        work_item_groups::create_group(
            &store,
            "demo",
            CreateWorkItemGroupRequest {
                key: "run-children".to_owned(),
                name: "Run children".to_owned(),
            },
            &attribution,
        )
        .await
        .unwrap();
        work_item_groups::assign_items(
            &store,
            "demo",
            "run-children",
            vec![child.id, second_child.id],
            &attribution,
        )
        .await
        .unwrap();

        let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "kind".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("child".to_owned()),
        })]);
        let configured = AutomationPostconditions {
            any_of: vec![
                AutomationOutcomeSet {
                    disposition: Some(ExpectedDisposition::Finished),
                    ..Default::default()
                },
                AutomationOutcomeSet {
                    disposition: Some(ExpectedDisposition::SuccessfulNonterminal),
                    attributed_events: vec![WorkItemEventType::LabelAdded],
                    labels: vec![LabelAssertion {
                        assertion: LabelAssertionKind::Added,
                        key: "approved".to_owned(),
                        value: None,
                    }],
                    created_items: Some(CreatedItemAssertion {
                        count: Some(2),
                        selector: Some(selector),
                        ..Default::default()
                    }),
                    created_item_assertions: vec![CreatedItemAssertion {
                        at_least: Some(2),
                        ..Default::default()
                    }],
                    created_items_share_group: true,
                    workspace_changes: Some(WorkspaceAssertion::Required),
                },
            ],
        };

        let result = evaluate(
            &store,
            "demo",
            run_id,
            Some(&baseline),
            Some(&configured),
            AgentCommitOutcome::Committed,
        )
        .await
        .unwrap();
        assert_that!(&(result.status)).is_equal_to(SemanticPostconditionStatus::Passed);
        assert_that!(&(result.failures.is_empty())).is_true();
    }

    #[tokio::test]
    async fn label_transitions_must_be_attributed_to_the_run() {
        let (_temp, store) = test_store().await;
        let run_id = insert_run(&store).await;
        let baseline = base_item(&store).await;
        item_label_service::add_label(
            &store,
            "demo",
            baseline.id,
            "external".to_owned(),
            None,
            None,
        )
        .await
        .unwrap();
        let configured = AutomationPostconditions {
            any_of: vec![AutomationOutcomeSet {
                labels: vec![LabelAssertion {
                    assertion: LabelAssertionKind::Added,
                    key: "external".to_owned(),
                    value: None,
                }],
                ..Default::default()
            }],
        };

        let result = evaluate(
            &store,
            "demo",
            run_id,
            Some(&baseline),
            Some(&configured),
            AgentCommitOutcome::SkippedNoChanges,
        )
        .await
        .unwrap();
        assert_that!(&(result.status)).is_equal_to(SemanticPostconditionStatus::Failed);
        assert_that!(&(result.failures[0].assertion)).is_equal_to("label_added");
    }

    #[tokio::test]
    async fn dispositions_are_derived_only_from_run_attributed_events() {
        let (_temp, store) = test_store().await;
        let project_id = projects::project_id(&store, "demo").await.unwrap();
        let item = base_item(&store).await;
        for (event_type, expected) in [
            (
                WorkItemEventType::ItemFinished,
                ExpectedDisposition::Finished,
            ),
            (
                WorkItemEventType::ItemReleased,
                ExpectedDisposition::Released,
            ),
            (
                WorkItemEventType::FeedbackRequested,
                ExpectedDisposition::FeedbackRequested,
            ),
        ] {
            let run_id = insert_run(&store).await;
            work_item_events::record_event_with_attribution_in_tx(
                store.db().as_ref(),
                project_id,
                Some(item.id),
                event_type,
                "transition",
                EventAttribution {
                    actor_type: Some(AuthorType::Agent),
                    actor_id: Some("agent-test"),
                    agent_run_id: Some(run_id),
                },
            )
            .await
            .unwrap();
            let configured = AutomationPostconditions {
                any_of: vec![AutomationOutcomeSet {
                    disposition: Some(expected),
                    ..Default::default()
                }],
            };
            assert_that!(
                &(evaluate(
                    &store,
                    "demo",
                    run_id,
                    Some(&item),
                    Some(&configured),
                    AgentCommitOutcome::SkippedNoChanges,
                )
                .await
                .unwrap()
                .status)
            )
            .is_equal_to(SemanticPostconditionStatus::Passed);
        }
    }
}
