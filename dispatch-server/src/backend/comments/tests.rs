use crate::backend::storage::Store;
use dispatch_types::*;

use assertr::prelude::*;
use tempfile::TempDir;

use super::*;
use crate::backend::{items::CreateWorkItem, projects::CreateProject};

async fn test_store() -> (TempDir, Store, i64) {
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
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Comment target".to_owned(),
            description: "Collect comments".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    (temp, store, item.id)
}

#[tokio::test]
async fn comments_append_in_created_order() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, item_id) = test_store().await;

    crate::backend::comments::tests::service(&store, event_bus.clone())
        .add(
            crate::backend::comments::CommentTarget::ProjectItem {
                project: "demo",
                item_id,
            },
            AddCommentRequest {
                author_type: AuthorType::User,
                author_name: Some("Lukas".to_owned()),
                body: "First".to_owned(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::comments::tests::service(&store, event_bus.clone())
        .add(
            crate::backend::comments::CommentTarget::ProjectItem {
                project: "demo",
                item_id,
            },
            AddCommentRequest {
                author_type: AuthorType::Agent,
                author_name: Some("codex".to_owned()),
                body: "Second".to_owned(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let comments =
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", item_id)
            .await
            .unwrap();
    let item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item_id)
            .await
            .unwrap();

    assert_that!(&(comments.len())).is_equal_to(2);
    assert_that!(&(comments[0].body)).is_equal_to("First");
    assert_that!(&(comments[1].author_type)).is_equal_to(AuthorType::Agent);
    assert_that!(&(item.comment_count)).is_equal_to(2);
    assert_that!(&(item.version)).is_equal_to(3);

    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item_id), None)
            .await
            .unwrap();
    let comment_events = events
        .iter()
        .filter(|event| event.event_type == WorkItemEventType::CommentAdded)
        .collect::<Vec<_>>();

    assert_that!(&(comment_events.len())).is_equal_to(2);
    assert_that!(
        &(comment_events.iter().all(|event| {
            event.work_item_id == Some(item_id)
                && event.body == "Added comment"
                && event.actor_type.is_none()
                && event.actor_id.is_none()
                && event.agent_run_id.is_none()
        }))
    )
    .is_true();
}

#[tokio::test]
async fn missing_project_comment_is_rejected() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, item_id) = test_store().await;

    let err = crate::backend::comments::tests::service(&store, event_bus.clone())
        .add(
            crate::backend::comments::CommentTarget::ProjectItem {
                project: "missing",
                item_id,
            },
            AddCommentRequest {
                author_type: AuthorType::User,
                author_name: None,
                body: "Nope".to_owned(),
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("project 'missing' does not exist"))).is_true();
}

pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::CommentService> {
    use crate::backend::{
        attribution::{repository::AttributionRepository, service::AttributionService},
        projects::repository::ProjectRepository,
        storage::TransactionManager,
    };
    use std::sync::Arc;
    let transactions = Arc::new(TransactionManager::new(store));
    let projects = Arc::new(ProjectRepository::new(store.db()));
    let attribution = Arc::new(AttributionService::new(
        transactions.clone(),
        projects.clone(),
        Arc::new(AttributionRepository::new()),
    ));
    Arc::new(super::service::CommentService::new(
        transactions,
        projects,
        Arc::new(super::repository::CommentRepository),
        attribution,
        events,
    ))
}

pub(crate) async fn application() -> (TempDir, crate::backend::application::Application, i64, i64) {
    let (temp, store, source_id) = test_store().await;
    let app =
        crate::backend::application::Application::from_store(store, "http://127.0.0.1:4000".into());
    let target =
        crate::backend::items::creation::tests::service(&app.state.store, app.state.events.clone())
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
                CreateWorkItem {
                    title: "Target".into(),
                    description: "Target item".into(),
                    state: "open".into(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: vec![],
                },
                Default::default(),
            )
            .await
            .unwrap();
    (temp, app, source_id, target.id)
}

#[tokio::test]
async fn failed_comment_history_rolls_back_comment_and_version_without_notifications() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, item_id, _) = application().await;
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER fail_comment_history BEFORE INSERT ON work_item_events WHEN NEW.event_type = 'comment_added' BEGIN SELECT RAISE(ABORT, 'injected history failure'); END".to_owned())).await.unwrap();
    let before = crate::backend::items::tests::service(
        &app.state.store,
        crate::backend::events::UiEventBus::new(),
    )
    .get("demo", item_id)
    .await
    .unwrap();
    let mut events = app.state.events.subscribe();
    let result = app
        .state
        .comments
        .add(
            CommentTarget::ProjectItem {
                project: "demo",
                item_id,
            },
            AddCommentRequest {
                author_type: AuthorType::User,
                author_name: None,
                body: "Must roll back".into(),
            },
            Default::default(),
        )
        .await;
    assert_that!(&result.is_err()).is_true();
    let after = crate::backend::items::tests::service(
        &app.state.store,
        crate::backend::events::UiEventBus::new(),
    )
    .get("demo", item_id)
    .await
    .unwrap();
    assert_that!(&after.version).is_equal_to(before.version);
    assert_that!(&after.updated_at).is_equal_to(before.updated_at);
    assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()).is_empty();
    assert_that!(&events.try_recv().is_err()).is_true();
    let history = crate::backend::items::tests::service(
        &app.state.store,
        crate::backend::events::UiEventBus::new(),
    )
    .events("demo", Some(item_id), None)
    .await
    .unwrap();
    assert_that!(
        &history
            .iter()
            .any(|event| event.event_type == WorkItemEventType::CommentAdded)
    )
    .is_false();
}
