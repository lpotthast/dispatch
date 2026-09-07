use crate::backend::{attribution::model::RequestAttribution, storage::Store};
use assertr::prelude::*;
use dispatch_types::CreateWorkItemGroupRequest;
use dispatch_types::WorkItemView;
use tempfile::TempDir;

use crate::backend::{items::CreateWorkItem, projects::CreateProject};

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

async fn create_test_item(store: &Store, project: &str, title: &str) -> WorkItemView {
    crate::backend::items::creation::tests::service(
        store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name(project),
        CreateWorkItem {
            title: title.to_owned(),
            description: format!("Test item {title}"),
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
async fn groups_are_idempotent_and_assignment_updates_item_views() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let attribution = RequestAttribution::default();
    let created = crate::backend::items::groups::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        "demo",
        CreateWorkItemGroupRequest {
            key: "review-42".to_owned(),
            name: "Review 42".to_owned(),
        },
        crate::backend::attribution::model::AttributionInput {
            agent_id: attribution.agent_id.clone(),
            agent_run_id: attribution.agent_run_id,
        },
    )
    .await
    .unwrap();
    let repeated = crate::backend::items::groups::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        "demo",
        CreateWorkItemGroupRequest {
            key: "review-42".to_owned(),
            name: "Review 42".to_owned(),
        },
        crate::backend::attribution::model::AttributionInput {
            agent_id: attribution.agent_id.clone(),
            agent_run_id: attribution.agent_run_id,
        },
    )
    .await
    .unwrap();
    assert_that!(&(created.id)).is_equal_to(repeated.id);

    let first = create_test_item(&store, "demo", "First").await;
    let second = create_test_item(&store, "demo", "Second").await;
    let assigned = crate::backend::items::groups::tests::service(&store, event_bus.clone())
        .assign(
            "demo",
            "review-42",
            vec![first.id, second.id, first.id],
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
        )
        .await
        .unwrap();
    assert_that!(&(assigned.item_count)).is_equal_to(2);
    let first =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", first.id)
            .await
            .unwrap();
    assert_that!(&(first.work_group.unwrap().key)).is_equal_to("review-42");
    assert_that!(&(first.version)).is_equal_to(2);
    assert_that!(
        &(crate::backend::items::groups::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list("demo", Default::default())
        .await
        .unwrap()[0]
            .item_count)
    )
    .is_equal_to(2);
}

#[tokio::test]
async fn assignment_is_project_scoped_and_atomic_on_conflict() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let attribution = RequestAttribution::default();
    for key in ["first", "second"] {
        crate::backend::items::groups::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(
            "demo",
            CreateWorkItemGroupRequest {
                key: key.to_owned(),
                name: key.to_owned(),
            },
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
        )
        .await
        .unwrap();
    }
    let available = create_test_item(&store, "demo", "Available").await;
    let occupied = create_test_item(&store, "demo", "Occupied").await;
    crate::backend::items::groups::tests::service(&store, event_bus.clone())
        .assign(
            "demo",
            "second",
            vec![occupied.id],
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
        )
        .await
        .unwrap();

    let error = crate::backend::items::groups::tests::service(&store, event_bus.clone())
        .assign(
            "demo",
            "first",
            vec![available.id, occupied.id],
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
        )
        .await
        .unwrap_err();
    assert_that!(&(error.to_string().contains("already belongs"))).is_true();
    let available =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", available.id)
            .await
            .unwrap();
    assert_that!(&(available.work_group.is_none())).is_true();
    assert_that!(&(available.version)).is_equal_to(1);

    let other = create_test_item(&store, "other", "Other").await;
    let error = crate::backend::items::groups::tests::service(&store, event_bus.clone())
        .assign(
            "demo",
            "first",
            vec![other.id],
            crate::backend::attribution::model::AttributionInput {
                agent_id: attribution.agent_id.clone(),
                agent_run_id: attribution.agent_run_id,
            },
        )
        .await
        .unwrap_err();
    assert_that!(&(error.to_string().contains("does not exist in this project"))).is_true();
}
pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::GroupService> {
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
    Arc::new(super::service::GroupService::new(
        transactions,
        projects,
        Arc::new(super::repository::GroupRepository),
        attribution,
        events,
    ))
}

#[tokio::test]
async fn assignment_history_failure_rolls_back_every_item_and_group_count() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, first_id, second_id) = crate::backend::comments::tests::application().await;
    let group = app
        .state
        .groups
        .create(
            "demo",
            CreateWorkItemGroupRequest {
                key: "batch".into(),
                name: "Batch".into(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let before = app.state.items.list("demo", None).await.unwrap();
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        format!("CREATE TRIGGER fail_group_history BEFORE INSERT ON work_item_events WHEN NEW.work_item_id = {second_id} AND NEW.event_type = 'item_updated' BEGIN SELECT RAISE(ABORT, 'injected second-item history failure'); END"))).await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.state.groups.assign(
            "demo",
            &group.key,
            vec![first_id, second_id],
            Default::default(),
        ),
    )
    .await
    .unwrap();
    assert_that!(&result.is_err()).is_true();
    assert_that!(&app.state.items.list("demo", None).await.unwrap()).is_equal_to(before);
    assert_that!(
        &app.state
            .groups
            .list("demo", Default::default())
            .await
            .unwrap()[0]
            .item_count
    )
    .is_equal_to(0);
    assert_that!(
        &app.state
            .items
            .events("demo", Some(first_id), None)
            .await
            .unwrap()
            .len()
    )
    .is_equal_to(1);
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_group_history".to_owned(),
        ))
        .await
        .unwrap();
    let assigned = app
        .state
        .groups
        .assign(
            "demo",
            &group.key,
            vec![first_id, second_id],
            Default::default(),
        )
        .await
        .unwrap();
    assert_that!(&assigned.item_count).is_equal_to(2);
    for id in [first_id, second_id] {
        let item = app.state.items.get("demo", id).await.unwrap();
        assert_that!(&item.version).is_equal_to(2);
        assert_that!(&item.work_group.unwrap().id).is_equal_to(group.id);
        assert_that!(&matches!(events.try_recv().unwrap(),dispatch_types::UiEvent::WorkItemChanged{item_id,..} if item_id==id)).is_true();
    }
    assert_that!(&events.try_recv().is_err()).is_true();
}
