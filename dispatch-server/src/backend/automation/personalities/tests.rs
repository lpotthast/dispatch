use assertr::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use tempfile::TempDir;

use super::{policy::DEFAULT_PERSONALITY_NAME, repository::ensure_default_personality_in_conn};
use crate::backend::{
    entities::personality::PersonalityActiveModel,
    projects::repository::ProjectRepository,
    storage::{Store, utc_now},
};
use crate::{
    backend::{entities::automation_trigger, projects::CreateProject},
    shared::view_models::{AutomationActivation, AutomationEffect, AutomationRunMutability},
};

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

#[tokio::test]
async fn new_project_gets_empty_default_personality() {
    let (_temp, store) = test_store().await;

    let personalities = service(&store).list("demo").await.unwrap();

    assert_that!(&(personalities.len())).is_equal_to(1);
    assert_that!(&(personalities[0].name)).is_equal_to(DEFAULT_PERSONALITY_NAME);
    assert_that!(&(personalities[0].personality_description)).is_equal_to("");
}

#[tokio::test]
async fn validation_rejects_cross_project_personality() {
    let (temp, store) = test_store().await;
    crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
        .create(CreateProject {
            name: "other".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    let demo_project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    let other_default = service(&store).list("other").await.unwrap()[0].id;

    let err = service(&store)
        .get_by_id(demo_project_id, other_default)
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("does not exist in this project"))).is_true();
}

#[tokio::test]
async fn delete_rejects_default_and_referenced_personality() {
    let (_temp, store) = test_store().await;
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    let default = ensure_default_personality_in_conn(store.db().as_ref(), project_id)
        .await
        .unwrap();
    let default_err = service(&store)
        .validate_delete(project_id, default.id)
        .await
        .unwrap_err();
    assert_that!(&(default_err.to_string().contains("cannot be deleted"))).is_true();

    let now = utc_now();
    let custom = PersonalityActiveModel {
        project_id: Set(project_id),
        name: Set("Review".to_owned()),
        personality_description: Set("Review carefully.".to_owned()),
        created_at: Set(now.clone()),
        updated_at: Set(now.clone()),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .unwrap();
    automation_trigger::ActiveModel {
        project_id: Set(project_id),
        name: Set("Review work".to_owned()),
        enabled: Set(true),
        activation: Set(AutomationActivation::WorkItem.as_storage().to_owned()),
        effect: Set(AutomationEffect::ConsumeWork.as_storage().to_owned()),
        schedule: Set("@every 15s".to_owned()),
        tool_name: Set("codex".to_owned()),
        mutability: Set(AutomationRunMutability::ReadOnly.as_storage().to_owned()),
        personality_id: Set(Some(custom.id)),
        prompt: Set(String::new()),
        work_item_selector: Set(Some(
            r#"{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}}]}"#
                .to_owned(),
        )),
        priority: Set(0),
        evaluation_count: Set(0),
        pending_evaluation_count: Set(0),
        last_evaluation_queued_at: Set(None),
        last_evaluated_at: Set(None),
        next_evaluation_at: Set(None),
        last_event_id: Set(None),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .unwrap();

    let err = service(&store)
        .validate_delete(project_id, custom.id)
        .await
        .unwrap_err();
    assert_that!(&(err.to_string().contains("referenced by automation trigger"))).is_true();
}

pub(crate) fn service(
    store: &crate::backend::storage::Store,
) -> std::sync::Arc<super::service::PersonalityService> {
    use std::sync::Arc;
    Arc::new(super::service::PersonalityService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(crate::backend::projects::repository::ProjectRepository::new(store.db())),
        Arc::new(super::repository::PersonalityRepository),
        crate::backend::events::UiEventBus::new(),
    ))
}

use crate::backend::projects::ProjectReference;
use dispatch_types::{AutomationPersonalityInput, RevisionChangeOperation};
fn input(name: &str, description: &str) -> AutomationPersonalityInput {
    AutomationPersonalityInput {
        key: String::new(),
        name: name.into(),
        description: description.into(),
    }
}

#[tokio::test]
async fn every_revision_mutation_rolls_back_when_history_insertion_fails() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let service = &app.state.personalities;
    let created = service
        .create(ProjectReference::Name("demo"), input("Review", "first"))
        .await
        .unwrap();
    let revision = created.current_revision_id.unwrap();
    let edited = service
        .update(
            ProjectReference::Name("demo"),
            created.id,
            input("Review", "second"),
        )
        .await
        .unwrap();
    let history =
        serde_json::to_value(service.revisions("demo", created.id).await.unwrap()).unwrap();
    let db = app.state.store.db();
    db.execute(Statement::from_string(DbBackend::Sqlite, "CREATE TRIGGER fail_personality_history BEFORE INSERT ON personality_revisions BEGIN SELECT RAISE(FAIL, 'history unavailable'); END".to_owned())).await.unwrap();
    let mut events = app.state.events.subscribe();
    assert_that!(
        &service
            .create(ProjectReference::Name("demo"), input("New", ""))
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&service.get("demo", "New").await.is_err()).is_true();
    assert_that!(
        &service
            .update(
                ProjectReference::Name("demo"),
                created.id,
                input("Changed", "third")
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&service.restore("demo", created.id, revision).await.is_err()).is_true();
    assert_that!(&serde_json::to_value(service.get("demo", "Review").await.unwrap()).unwrap())
        .is_equal_to(serde_json::to_value(&edited).unwrap());
    assert_that!(
        &serde_json::to_value(service.revisions("demo", created.id).await.unwrap()).unwrap()
    )
    .is_equal_to(history);
    db.execute(Statement::from_string(DbBackend::Sqlite, format!("UPDATE personalities SET managed_bundle_key = 'bundle', managed_object_key = 'review' WHERE id = {}", created.id))).await.unwrap();
    assert_that!(&service.detach("demo", created.id).await.is_err()).is_true();
    assert_that!(
        &service
            .get("demo", "Review")
            .await
            .unwrap()
            .managed_bundle_key
            .as_deref()
    )
    .is_equal_to(Some("bundle"));
    assert_that!(&events.try_recv().is_err()).is_true();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "DROP TRIGGER fail_personality_history".to_owned(),
    ))
    .await
    .unwrap();
    let detached = service.detach("demo", created.id).await.unwrap();
    assert_that!(&detached.managed_bundle_key).is_none();
    assert_that!(&service.revisions("demo", created.id).await.unwrap()[0].operation)
        .is_equal_to(RevisionChangeOperation::Detach);
    events.try_recv().unwrap();
    let restored = service.restore("demo", created.id, revision).await.unwrap();
    assert_that!(&restored.personality_description).is_equal_to("first");
    assert_that!(&restored.current_revision_id).is_not_equal_to(Some(revision));
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn crudkit_and_operator_personality_writes_share_revisions_and_notifications() {
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = super::transport::api::routes()
        .merge(super::transport::crud::routes())
        .layer(axum::Extension(app.state.clone()))
        .layer(axum::Extension(app.contexts.personality.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let mut events = app.state.events.subscribe();
    let response = client.post(format!("{url}/api/personalities/crud/create-one")).json(&serde_json::json!({"entity":{"project_id":project_id,"name":"  Review  ","personality_description":"first"}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let body = response.json::<serde_json::Value>().await.unwrap();
    let id = body["entity"]["id"].as_i64().unwrap();
    let record = app
        .state
        .personalities
        .get("demo", &id.to_string())
        .await
        .unwrap();
    assert_that!(&record.name).is_equal_to("Review");
    assert_that!(&body["entity"]["current_revision_id"].as_i64())
        .is_equal_to(record.current_revision_id);
    let revision = record.current_revision_id.unwrap();
    events.try_recv().unwrap();
    let base = format!("{url}/operator/api/projects/demo/automation/personalities/{id}");
    let response = client
        .put(&base)
        .json(&input("Review", "second"))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    let response = client
        .post(format!("{base}/restore"))
        .json(&serde_json::json!({"revision_id":revision}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    assert_that!(
        &app.state
            .personalities
            .get("demo", "Review")
            .await
            .unwrap()
            .personality_description
    )
    .is_equal_to("first");
    let history = app.state.personalities.revisions("demo", id).await.unwrap();
    assert_that!(
        &history
            .iter()
            .map(|revision| revision.operation)
            .collect::<Vec<_>>()
    )
    .is_equal_to(vec![
        RevisionChangeOperation::Restore,
        RevisionChangeOperation::Update,
        RevisionChangeOperation::Create,
    ]);
    let condition =
        serde_json::json!({"All":[{"column_name":"id","operator":"=","value":{"I64":id}}]});
    let response = client.post(format!("{url}/api/personalities/crud/update-one")).json(&serde_json::json!({"condition":condition,"entity":{"name":"Review","personality_description":"third"}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    assert_that!(
        &app.state
            .personalities
            .get("demo", "Review")
            .await
            .unwrap()
            .personality_description
    )
    .is_equal_to("third");
    let response = client
        .post(format!("{url}/api/personalities/crud/delete-one"))
        .json(&serde_json::json!({"condition":condition}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    assert_that!(&app.state.personalities.get("demo", "Review").await.is_err()).is_true();
    let default = app
        .state
        .personalities
        .get("demo", "Default")
        .await
        .unwrap();
    let response = client
        .delete(format!(
            "{url}/operator/api/projects/demo/automation/personalities/{}",
            default.id
        ))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::BAD_REQUEST);
    assert_that!(&events.try_recv().is_err()).is_true();
    server.abort();
}

#[tokio::test]
async fn restore_validates_project_revision_ownership_and_name_conflicts() {
    let (temp, store) = test_store().await;
    let service = service(&store);
    crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
        .create(CreateProject {
            name: "other".into(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    let first = service
        .create(ProjectReference::Name("demo"), input("First", "first"))
        .await
        .unwrap();
    let revision = first.current_revision_id.unwrap();
    let other = service
        .create(ProjectReference::Name("other"), input("Other", "other"))
        .await
        .unwrap();
    assert_that!(&service.restore("other", first.id, revision).await.is_err()).is_true();
    assert_that!(
        &service
            .restore("demo", first.id, other.current_revision_id.unwrap())
            .await
            .is_err()
    )
    .is_true();
    service
        .update(
            ProjectReference::Name("demo"),
            first.id,
            input("Renamed", "second"),
        )
        .await
        .unwrap();
    service
        .create(ProjectReference::Name("demo"), input("First", "different"))
        .await
        .unwrap();
    assert_that!(&service.restore("demo", first.id, revision).await.is_err()).is_true();
    assert_that!(
        &service
            .get("demo", "Renamed")
            .await
            .unwrap()
            .personality_description
    )
    .is_equal_to("second");
    assert_that!(&service.revisions("demo", first.id).await.unwrap().len()).is_equal_to(2);
}
