use assertr::prelude::*;
use axum::http::HeaderValue;
use dispatch_types::AutomationRunMutability;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, TransactionTrait};
use tempfile::TempDir;

use super::transport::{AGENT_ID_HEADER, AGENT_RUN_ID_HEADER};
use crate::backend::{
    attribution::{repository::AttributionRepository, service::AttributionService, transport},
    execution::identity as agent_ids,
    projects::repository::ProjectRepository,
    storage::Store,
    storage::TransactionManager,
};
use axum::http::HeaderMap;
use dispatch_types::{AgentRunKind, AgentRunPurposeV1, AgentRunStatus, WorkItemOriginKind};
use std::sync::Arc;

fn service(store: &Store) -> AttributionService {
    AttributionService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(AttributionRepository::new()),
    )
}
use crate::backend::{
    entities::agent_run::AgentRunActiveModel,
    projects::CreateProject,
    runs::launch::model::{AgentCapabilitySetV1, AgentLaunchTargetV1},
    runs::launch::repository::{insert_contract_in_tx, mark_spawned_in_tx, mark_terminal_in_tx},
    storage::utc_now,
};

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    for name in ["demo", "other"] {
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: name.to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
    }
    (temp, store)
}

async fn insert_run(store: &Store, project_name: &str) -> i64 {
    let project_id = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .id(project_name)
        .await
        .unwrap();
    let now = utc_now();
    AgentRunActiveModel {
        project_id: Set(project_id),
        trigger_name: Set(Some("lineage-rule".to_owned())),
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

async fn insert_contracted_run(
    store: &Store,
    project_name: &str,
    mutability: AutomationRunMutability,
    status: AgentRunStatus,
    resolved: bool,
) -> i64 {
    let project_id = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .id(project_name)
        .await
        .unwrap();
    let now = utc_now();
    let txn = store.db().begin().await.unwrap();
    let run = AgentRunActiveModel {
        project_id: Set(project_id),
        run_kind: Set(AgentRunKind::Task.as_storage().to_owned()),
        purpose: Set(Some(AgentRunPurposeV1::Ordinary.as_storage().to_owned())),
        tool_name: Set("codex".to_owned()),
        mutability: Set(mutability.as_storage().to_owned()),
        status: Set(status.as_storage().to_owned()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        created_at: Set(now.clone()),
        updated_at: Set(now.clone()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .unwrap();
    let target = if resolved {
        AgentLaunchTargetV1::none()
    } else {
        AgentLaunchTargetV1::next_open("open").unwrap()
    };
    insert_contract_in_tx(
        &txn,
        project_id,
        run.id,
        AgentRunPurposeV1::Ordinary,
        &target,
        &AgentCapabilitySetV1::ordinary(),
        &now,
    )
    .await
    .unwrap();
    if resolved {
        mark_spawned_in_tx(&txn, project_id, run.id, &now)
            .await
            .unwrap();
        if status != AgentRunStatus::Running {
            mark_terminal_in_tx(&txn, project_id, run.id, &now)
                .await
                .unwrap();
        }
    }
    txn.commit().await.unwrap();
    run.id
}

fn headers(agent_id: Option<&str>, run_id: Option<i64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(agent_id) = agent_id {
        headers.insert(AGENT_ID_HEADER, HeaderValue::from_str(agent_id).unwrap());
    }
    if let Some(run_id) = run_id {
        headers.insert(
            AGENT_RUN_ID_HEADER,
            HeaderValue::from_str(&run_id.to_string()).unwrap(),
        );
    }
    headers
}

#[tokio::test]
async fn request_attribution_validates_run_project_and_derived_agent() {
    let (_temp, store) = test_store().await;
    let run_id = insert_run(&store, "demo").await;
    let agent_id = agent_ids::dispatch_run_agent_id(run_id);

    let operator = transport::from_headers(&service(&store), "demo", &HeaderMap::new())
        .await
        .unwrap();
    assert_that!(&(operator.agent_id.is_none())).is_true();

    let missing_agent =
        transport::from_headers(&service(&store), "demo", &headers(None, Some(run_id)))
            .await
            .unwrap_err();
    assert_that!(&(missing_agent.to_string().contains("requires"))).is_true();

    let wrong_agent = transport::from_headers(
        &service(&store),
        "demo",
        &headers(Some("agent-wrong"), Some(run_id)),
    )
    .await
    .unwrap_err();
    assert_that!(&(wrong_agent.to_string().contains("does not match"))).is_true();

    let cross_project = transport::from_headers(
        &service(&store),
        "other",
        &headers(Some(&agent_id), Some(run_id)),
    )
    .await
    .unwrap_err();
    assert_that!(
        &(cross_project
            .to_string()
            .contains("does not exist in this project"))
    )
    .is_true();

    let attribution = transport::from_headers(
        &service(&store),
        "demo",
        &headers(Some(&agent_id), Some(run_id)),
    )
    .await
    .unwrap();
    assert_that!(&(attribution.agent_id.as_deref())).is_equal_to(Some(agent_id.as_str()));
    assert_that!(&(attribution.agent_run_id)).is_equal_to(Some(run_id));
    assert_that!(&(attribution.item_origin().kind)).is_equal_to(WorkItemOriginKind::AgentRun);
    assert_that!(&(attribution.item_origin().agent_run_id)).is_equal_to(Some(run_id));
    attribution.cross_check_agent_id(&agent_id).unwrap();
    assert_that!(&(attribution.cross_check_agent_id("agent-other").is_err())).is_true();
}

#[tokio::test]
async fn knowledge_agent_headers_require_a_project_scoped_persisted_run() {
    let (_temp, store) = test_store().await;
    let arbitrary = transport::from_knowledge_headers(
        &service(&store),
        "demo",
        &headers(Some("agent-arbitrary"), None),
    )
    .await
    .unwrap_err();
    assert_that!(&arbitrary.to_string()).contains(AGENT_RUN_ID_HEADER);

    let run_id = insert_contracted_run(
        &store,
        "demo",
        AutomationRunMutability::Mutating,
        AgentRunStatus::Running,
        true,
    )
    .await;
    let cross_project = transport::from_knowledge_headers(
        &service(&store),
        "other",
        &headers(
            Some(&agent_ids::dispatch_run_agent_id(run_id)),
            Some(run_id),
        ),
    )
    .await
    .unwrap_err();
    assert_that!(&cross_project.to_string()).contains("does not exist in this project");
}

#[tokio::test]
async fn nested_attribution_uses_uncommitted_scope_with_one_connection() {
    use super::model::AttributionInput;
    use crate::backend::entities::project::ProjectActiveModel;
    use sea_orm::EntityTrait;
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("single.sqlite3"), 1)
        .await
        .unwrap();
    let service = service(&store);
    let transaction = TransactionManager::new(&store).begin().await.unwrap();
    let project = ProjectActiveModel {
        name: Set("uncommitted".into()),
        display_name: Set("Uncommitted".into()),
        created_at: Set(utc_now()),
        updated_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(transaction.connection())
    .await
    .unwrap();
    let run = AgentRunActiveModel {
        project_id: Set(project.id),
        tool_name: Set("codex".into()),
        run_kind: Set("task".into()),
        status: Set("running".into()),
        mutability: Set("read_only".into()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        created_at: Set(utc_now()),
        updated_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(transaction.connection())
    .await
    .unwrap();
    let input = AttributionInput {
        agent_id: Some(agent_ids::dispatch_run_agent_id(run.id)),
        agent_run_id: Some(run.id),
    };
    let attribution = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        service.validate_in(&transaction, "uncommitted", input, false),
    )
    .await
    .unwrap()
    .unwrap();
    assert_that!(&attribution.agent_run_id).is_equal_to(Some(run.id));
    transaction.rollback().await.unwrap();
    assert_that!(
        &crate::backend::entities::project::Project::find_by_id(project.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .is_none()
    )
    .is_true();
    assert_that!(
        &crate::backend::entities::agent_run::AgentRun::find_by_id(run.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .is_none()
    )
    .is_true();
}
