use assertr::prelude::*;
use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use tempfile::TempDir;

use crate::backend::{projects::repository::ProjectRepository, storage::Store};
use crate::{
    backend::{
        attribution::model::RequestAttribution,
        entities::{
            agent_run::AgentRunActiveModel,
            work_item_origin::{WorkItemOrigin, WorkItemOriginActiveModel},
        },
        items::CreateWorkItem,
        items::events::{model::EventAttribution, repository as work_item_events},
        projects::CreateProject,
        storage::utc_now,
    },
    shared::view_models::{
        AuthorType, AutomationOutcomeSet, CreateWorkItemGroupRequest, CreateWorkItemLabelRequest,
        CreatedItemAssertion, WorkItemOriginKind,
    },
};
use dispatch_types::{
    AgentCommitOutcome, AutomationPostconditions, ExpectedDisposition, SemanticPostconditionStatus,
    WorkItemEventType, WorkItemView,
};
use dispatch_types::{LabelAssertion, LabelAssertionKind, WorkspaceAssertion};

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("dispatch.sqlite3"), 1)
        .await
        .unwrap();
    crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    (temp, store)
}

async fn insert_run(store: &Store) -> i64 {
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
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
    crate::backend::items::creation::tests::service(
        store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Postcondition target".to_owned(),
            description: "Evaluate semantic outcomes".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn unconfigured_postconditions_are_observational() {
    let (_temp, store) = test_store().await;
    let run_id = insert_run(&store).await;
    let result = service(&store)
        .evaluate("demo", run_id, None, None, AgentCommitOutcome::NotRequired)
        .await
        .unwrap();
    assert_that!(&(result.status)).is_equal_to(SemanticPostconditionStatus::NotConfigured);
    assert_that!(&(result.failures.is_empty())).is_true();
}

#[tokio::test]
async fn alternative_outcome_can_require_attributed_labels_created_items_and_workspace() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let run_id = insert_run(&store).await;
    let baseline = base_item(&store).await;
    let agent_id = format!("agent-run-{run_id}");
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            baseline.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "approved".to_owned(),
                value: None,
            },
            None,
            crate::backend::attribution::model::AttributionInput {
                agent_id: Some(format!("dispatch-run-{run_id}")),
                agent_run_id: Some(run_id),
            },
        )
        .await
        .unwrap();
    let child = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
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
        Default::default(),
    )
    .await
    .unwrap();
    let second_child = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
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
        Default::default(),
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
    crate::backend::items::groups::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        "demo",
        CreateWorkItemGroupRequest {
            key: "run-children".to_owned(),
            name: "Run children".to_owned(),
        },
        crate::backend::attribution::model::AttributionInput {
            agent_id: attribution.agent_id.clone(),
            agent_run_id: attribution.agent_run_id,
        },
    )
    .await
    .unwrap();
    crate::backend::items::groups::tests::service(&store, event_bus.clone())
        .assign(
            "demo",
            "run-children",
            vec![child.id, second_child.id],
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
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

    let result = service(&store)
        .evaluate(
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
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let run_id = insert_run(&store).await;
    let baseline = base_item(&store).await;
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            baseline.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "external".to_owned(),
                value: None,
            },
            None,
            Default::default(),
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

    let result = service(&store)
        .evaluate(
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
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
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
            &(service(&store)
                .evaluate(
                    "demo",
                    run_id,
                    Some(&item),
                    Some(&configured),
                    AgentCommitOutcome::SkippedNoChanges
                )
                .await
                .unwrap()
                .status)
        )
        .is_equal_to(SemanticPostconditionStatus::Passed);
    }
}
pub(crate) fn service(store: &Store) -> std::sync::Arc<super::service::PostconditionService> {
    use std::sync::Arc;
    Arc::new(super::service::PostconditionService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(super::repository::PostconditionRepository),
        Arc::new(crate::backend::items::repository::ItemRepository),
    ))
}
