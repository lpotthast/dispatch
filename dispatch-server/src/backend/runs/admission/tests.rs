use super::{repository::RunAdmissionRepository, service::RunAdmissionService};
use crate::backend::{
    projects::repository::ProjectRepository,
    storage::{Store, TransactionManager},
};
use std::sync::Arc;
pub(crate) fn service(store: &Store) -> Arc<RunAdmissionService> {
    Arc::new(RunAdmissionService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(RunAdmissionRepository),
    ))
}

#[tokio::test]
async fn application_admission_is_shared_and_cancellation_releases_its_permit() {
    use assertr::prelude::*;
    use std::time::Duration;
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let (_other_temp, other, _, _) = crate::backend::comments::tests::application().await;
    let admission = app.state.run_admission.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _permit = admission.acquire().await;
        ready_tx.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    ready_rx.await.unwrap();
    assert_that!(
        &tokio::time::timeout(Duration::from_millis(20), app.state.run_admission.acquire())
            .await
            .is_err()
    )
    .is_true();
    let other_permit =
        tokio::time::timeout(Duration::from_secs(1), other.state.run_admission.acquire())
            .await
            .unwrap();
    drop(other_permit);
    task.abort();
    assert_that!(&task.await.unwrap_err().is_cancelled()).is_true();
    let permit = tokio::time::timeout(Duration::from_secs(1), app.state.run_admission.acquire())
        .await
        .unwrap();
    drop(permit);
}

#[tokio::test]
async fn admission_sees_uncommitted_runs_using_the_supplied_single_connection() {
    use crate::backend::{entities::agent_run::AgentRunActiveModel, storage::utc_now};
    use assertr::prelude::*;
    use dispatch_types::{AutomationExecutionPolicy, AutomationRunMutability};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use std::time::Duration;
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    app.state
        .projects
        .update_settings(
            "demo",
            crate::backend::projects::UpdateProjectSettings {
                max_read_only_agents: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let settings = app.state.projects.settings("demo").await.unwrap();
    let transaction = TransactionManager::new(&app.state.store)
        .begin()
        .await
        .unwrap();
    let now = utc_now();
    AgentRunActiveModel {
        project_id: Set(settings.project_id),
        tool_name: Set("codex".into()),
        mutability: Set("read_only".into()),
        status: Set("running".into()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(transaction.connection())
    .await
    .unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        app.state.run_admission.enforce_in(
            &transaction,
            "demo",
            &settings,
            AutomationRunMutability::ReadOnly,
            None,
            &AutomationExecutionPolicy::default(),
        ),
    )
    .await
    .expect("admission must reuse the supplied transaction")
    .unwrap_err();
    assert_that!(&error.to_string()).contains("limit is 1");
    transaction.rollback().await.unwrap();
    assert_that!(
        &app.state
            .run_admission
            .running_counts_for_project_id(settings.project_id)
            .await
            .unwrap()
            .total()
    )
    .is_equal_to(0);
}
