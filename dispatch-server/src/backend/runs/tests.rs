pub(crate) fn service(
    store: &crate::backend::storage::Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::RunService> {
    use std::sync::Arc;
    Arc::new(super::service::RunService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(crate::backend::projects::repository::ProjectRepository::new(store.db())),
        Arc::new(super::repository::RunRepository),
        crate::backend::items::claims::tests::service(store, events.clone()),
        Arc::new(super::queries::runtime::RunArtifacts),
        events,
    ))
}

#[tokio::test]
async fn terminal_contract_failure_rolls_back_claims_history_and_the_entire_cancel_batch() {
    use super::model::CreateRunConfig;
    use crate::backend::{
        items::CreateWorkItem, projects::ProjectReference, runs::launch::model::AgentLaunchTargetV1,
    };
    use assertr::prelude::*;
    use dispatch_types::{
        AgentRunKind, AgentRunPurposeV1, AgentRunStatus, AgentToolName, AutomationRunMutability,
    };
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let mut fixtures = Vec::new();
    for title in ["First", "Second"] {
        let item = app
            .state
            .item_creation
            .create(
                ProjectReference::Name("demo"),
                CreateWorkItem {
                    title: title.into(),
                    description: "Atomic run completion".into(),
                    state: "open".into(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: Vec::new(),
                },
                Default::default(),
            )
            .await
            .unwrap();
        let target = AgentLaunchTargetV1::specific(item.id, item.version).unwrap();
        let run = app
            .state
            .runs
            .create(
                project_id,
                CreateRunConfig {
                    tool: AgentToolName::Codex,
                    mutability: AutomationRunMutability::Mutating,
                    trigger: None,
                    personality_revision_id: None,
                    effective_timeout_seconds: 60,
                    effective_concurrency_group: None,
                    run_kind: AgentRunKind::Task,
                    purpose: AgentRunPurposeV1::Ordinary,
                    knowledge_job_id: None,
                    launch_target: &target,
                },
            )
            .await
            .unwrap();
        let agent = crate::backend::execution::identity::dispatch_run_agent_id(run.id);
        let claimed = app
            .state
            .claims
            .resolve_agent_run_target("demo", run.id, &agent, &target, None)
            .await
            .unwrap()
            .unwrap();
        assert_that!(&claimed.claim_source.as_ref().unwrap().run_id).is_equal_to(run.id);
        fixtures.push((run, claimed));
    }
    let history =
        serde_json::to_value(app.state.items.events("demo", None, None).await.unwrap()).unwrap();
    let mut events = app.state.events.subscribe();
    let sql = format!(
        "CREATE TRIGGER fail_terminal BEFORE UPDATE ON agent_run_launch_contracts WHEN NEW.run_id = {} AND NEW.state = 'terminal' BEGIN SELECT RAISE(FAIL, 'terminal history unavailable'); END",
        fixtures[0].0.id
    );
    app.state
        .store
        .db()
        .execute(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .unwrap();
    // The caller retains its pre-claim record; completion resolves current claim ownership itself.
    assert_that!(&fixtures[0].0.work_item_id).is_none();
    assert_that!(
        &app.state
            .runs
            .finish(
                fixtures[0].0.clone(),
                AgentRunStatus::Failed,
                None,
                "Execution failed".into()
            )
            .await
            .is_err()
    )
    .is_true();
    // Descending allocation order means the second run is changed before the first run's failure.
    assert_that!(&app.state.runs.cancel_project(project_id).await.is_err()).is_true();
    for (run, item) in &fixtures {
        assert_that!(
            &app.state
                .run_queries
                .get("demo", run.id)
                .await
                .unwrap()
                .status
        )
        .is_equal_to(AgentRunStatus::Running);
        assert_that!(
            &serde_json::to_value(app.state.items.get("demo", item.id).await.unwrap()).unwrap()
        )
        .is_equal_to(serde_json::to_value(item).unwrap());
    }
    assert_that!(
        &serde_json::to_value(app.state.items.events("demo", None, None).await.unwrap()).unwrap()
    )
    .is_equal_to(history);
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_terminal".to_owned(),
        ))
        .await
        .unwrap();
    let cancelled = app.state.runs.cancel_project(project_id).await.unwrap();
    assert_that!(&cancelled.len()).is_equal_to(2);
    for run in cancelled {
        assert_that!(&run.status).is_equal_to(AgentRunStatus::Cancelled);
        assert_that!(
            &app.state
                .items
                .get("demo", run.work_item_id.unwrap())
                .await
                .unwrap()
                .claimed_by
        )
        .is_none();
    }
    for _ in 0..4 {
        assert_that!(&events.try_recv().is_ok()).is_true();
    }
    assert_that!(&events.try_recv().is_err()).is_true();
    let late = app
        .state
        .runs
        .finish(
            fixtures[0].0.clone(),
            AgentRunStatus::Completed,
            Some(0),
            "Late process completion".into(),
        )
        .await
        .unwrap();
    assert_that!(&late.status).is_equal_to(AgentRunStatus::Cancelled);
    assert_that!(&events.try_recv().is_err()).is_true();
}

async fn administrative_run(
    app: &crate::backend::application::Application,
    item: Option<i64>,
) -> dispatch_types::AgentRunView {
    use dispatch_types::*;
    let target = match item {
        Some(id) => super::launch::model::AgentLaunchTargetV1::specific(
            id,
            app.state.items.get("demo", id).await.unwrap().version,
        )
        .unwrap(),
        None => super::launch::model::AgentLaunchTargetV1::none(),
    };
    let run = app
        .state
        .runs
        .create(
            app.state.projects.id("demo").await.unwrap(),
            super::model::CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: 60,
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &target,
            },
        )
        .await
        .unwrap();
    if item.is_some() {
        app.state
            .claims
            .resolve_agent_run_target(
                "demo",
                run.id,
                &crate::backend::execution::identity::dispatch_run_agent_id(run.id),
                &target,
                None,
            )
            .await
            .unwrap();
    }
    app.state.run_queries.get("demo", run.id).await.unwrap()
}
fn administration(
    app: &crate::backend::application::Application,
    artifacts: std::path::PathBuf,
) -> std::sync::Arc<super::administration::RunAdministrationService> {
    use std::sync::Arc;
    Arc::new(super::administration::RunAdministrationService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(
            &app.state.store,
        )),
        Arc::new(
            crate::backend::projects::repository::ProjectRepository::new(app.state.store.db()),
        ),
        Arc::new(super::repository::RunRepository),
        app.state.runs.clone(),
        app.state.jobs.clone(),
        app.state.run_admission.clone(),
        app.state.sessions.clone(),
        Arc::new(super::cleanup::RunCleanupRuntime::new(
            artifacts,
            std::sync::Arc::new(crate::backend::execution::workspaces::runs::RunWorkspaceRuntime),
        )),
    ))
}
#[tokio::test]
async fn administrative_deletion_rolls_back_claim_history_and_reopens_admission_on_failure() {
    use assertr::prelude::*;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (temp, app, item, _) = crate::backend::comments::tests::application().await;
    let run = administrative_run(&app, Some(item)).await;
    let before = serde_json::to_value(app.state.items.get("demo", item).await.unwrap()).unwrap();
    let history = serde_json::to_value(
        app.state
            .items
            .events("demo", Some(item), None)
            .await
            .unwrap(),
    )
    .unwrap();
    let admin = administration(&app, temp.path().join("run-artifacts"));
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER reject_run_delete BEFORE DELETE ON agent_runs BEGIN SELECT RAISE(ABORT,'injected deletion failure'); END".to_owned())).await.unwrap();
    assert_that!(&admin.delete(run.project_id, run.id).await.is_err()).is_true();
    assert_that!(&serde_json::to_value(app.state.items.get("demo", item).await.unwrap()).unwrap())
        .is_equal_to(before);
    assert_that!(
        &serde_json::to_value(
            app.state
                .items
                .events("demo", Some(item), None)
                .await
                .unwrap()
        )
        .unwrap()
    )
    .is_equal_to(history);
    assert_that!(
        &app.state
            .run_queries
            .get("demo", run.id)
            .await
            .unwrap()
            .status
    )
    .is_equal_to(dispatch_types::AgentRunStatus::Running);
    assert_that!(&events.try_recv().is_err()).is_true();
    drop(
        app.state
            .sessions
            .begin_run_deletion(run.project_id, run.id)
            .unwrap(),
    );
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_run_delete".to_owned(),
        ))
        .await
        .unwrap();
    assert_that!(&admin.delete(run.project_id, run.id).await.unwrap()).is_equal_to(1);
    assert_that!(&app.state.items.get("demo", item).await.unwrap().claimed_by).is_none();
    assert_that!(&app.state.run_queries.get("demo", run.id).await.is_err()).is_true();
    assert_that!(&events.try_recv().is_ok()).is_true();
    assert_that!(&events.try_recv().is_ok()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
}
#[tokio::test]
async fn administrative_cleanup_failure_preserves_the_record_for_retry() {
    use assertr::prelude::*;
    let (temp, app, _, _) = crate::backend::comments::tests::application().await;
    let run = administrative_run(&app, None).await;
    let path = temp.path().join("not-a-directory");
    std::fs::write(&path, "blocked").unwrap();
    let admin = administration(&app, path.clone());
    let mut events = app.state.events.subscribe();
    assert_that!(&admin.delete(run.project_id, run.id).await.is_err()).is_true();
    assert_that!(&app.state.run_queries.get("demo", run.id).await.is_ok()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    std::fs::remove_file(path).unwrap();
    assert_that!(&admin.delete(run.project_id, run.id).await.unwrap()).is_equal_to(1);
}
#[tokio::test]
async fn administrative_deletion_drains_registered_processes_and_rejects_late_registration() {
    use crate::backend::execution::sessions::ProcessSessionStart;
    use assertr::prelude::*;
    let (temp, app, _, _) = crate::backend::comments::tests::application().await;
    let run = administrative_run(&app, None).await;
    let start = || ProcessSessionStart {
        run_id: run.id,
        project_id: run.project_id,
        project_name: "demo".into(),
        tool_name: "codex".into(),
        command: String::new(),
        working_dir: String::new(),
    };
    let registration = app.state.sessions.begin(start(), &Default::default());
    let sessions = app.state.sessions.clone();
    let id = run.id;
    let process = tokio::spawn(async move {
        registration.wait_for_cancellation().await;
        sessions.finish(id);
    });
    let admin = administration(&app, temp.path().join("runs"));
    assert_that!(&admin.delete(run.project_id, run.id).await.unwrap()).is_equal_to(1);
    process.await.unwrap();
    let late = app.state.sessions.begin(start(), &Default::default());
    assert_that!(&late.is_registered()).is_false();
    assert_that!(&late.cancellation_requested()).is_true();
}
#[tokio::test]
async fn crudkit_run_metadata_and_deletion_use_authoritative_services() {
    use assertr::prelude::*;
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use dispatch_types::AgentToolName;
    let (_temp, app, item, _) = crate::backend::comments::tests::application().await;
    let run = administrative_run(&app, Some(item)).await;
    let mut events = app.state.events.subscribe();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/api/agent_runs/crud",
        listener.local_addr().unwrap()
    );
    let router =
        super::transport::crud::routes().layer(axum::Extension(app.contexts.agent_run.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::new();
    let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "id".into(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(run.id),
    })]);
    let metadata = super::model::RunMetadata {
        work_item_id: Some(item),
        tool: AgentToolName::Codex,
    };
    app.state
        .runs
        .update_metadata(run.project_id, run.id, metadata)
        .await
        .unwrap();
    events.try_recv().unwrap();
    let response=client.post(format!("{url}/update-one")).json(&serde_json::json!({"condition":condition,"entity":{"work_item_id":item,"tool_name":"codex"}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    let updated = app.state.run_queries.get("demo", run.id).await.unwrap();
    assert_that!(&updated.launch_target).is_equal_to(run.launch_target);
    assert_that!(&updated.status).is_equal_to(run.status);
    assert_that!(&updated.command).is_equal_to(run.command);
    let response=client.post(format!("{url}/update-one")).json(&serde_json::json!({"condition":condition,"entity":{"work_item_id":null,"tool_name":"codex"}})).send().await.unwrap();
    assert_that!(&response.status().is_server_error()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    let response=client.post(format!("{url}/create-one")).json(&serde_json::json!({"entity":{"project_id":run.project_id,"work_item_id":null,"tool_name":"codex"}})).send().await.unwrap();
    assert_that!(&response.status().is_server_error()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    let response = client
        .post(format!("{url}/delete-one"))
        .json(&serde_json::json!({"condition":condition}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    assert_that!(&app.state.run_queries.get("demo", run.id).await.is_err()).is_true();
    assert_that!(&app.state.items.get("demo", item).await.unwrap().claimed_by).is_none();
    server.abort();
}

#[tokio::test]
async fn stale_working_copy_scope_cannot_delete_a_project_or_run() {
    use assertr::prelude::*;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (temp, app, _, _) = crate::backend::comments::tests::application().await;
    let run = administrative_run(&app, None).await;
    let scope = crate::backend::projects::repository::ProjectRepository::new(app.state.store.db())
        .scope_by_id(run.project_id)
        .await
        .unwrap();
    let changed = temp.path().join("changed-workspace");
    std::fs::create_dir(&changed).unwrap();
    app.state
        .store
        .db()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE projects SET path=? WHERE id=?",
            [
                changed.to_string_lossy().to_string().into(),
                run.project_id.into(),
            ],
        ))
        .await
        .unwrap();
    let mut events = app.state.events.subscribe();
    assert_that!(&app.state.runs.delete_record(&scope, run.id).await.is_err()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    assert_that!(
        &app.state
            .project_deletion
            .delete_project(scope)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.projects.id("demo").await.unwrap()).is_equal_to(run.project_id);
    assert_that!(&app.state.run_queries.get("demo", run.id).await.is_ok()).is_true();
}
