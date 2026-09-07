use crate::backend::storage::Store;
use assertr::prelude::*;
use tempfile::TempDir;

use crate::backend::{items::CreateWorkItem, projects::CreateProject};
use crate::shared::view_models::{STATE_LABEL_KEY, WorkItemEventType};

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
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
async fn add_update_and_delete_label_touch_item_and_preserve_state_label() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Label item".to_owned(),
            description: "Exercise label service behavior".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let added = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "priority".to_owned(),
                value: Some("high".to_owned()),
            },
            Some(item.version),
            Default::default(),
        )
        .await
        .unwrap();
    let label_id = added
        .labels
        .iter()
        .find(|label| label.key == "priority")
        .unwrap()
        .id;

    let updated = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            item.id,
            label_id,
            dispatch_types::UpdateWorkItemLabelRequest {
                key: None,
                value: Some(Some("low".to_owned())),
                expect_version: Some(added.version),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let deleted = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .delete(
            "demo",
            item.id,
            label_id,
            Some(updated.version),
            Default::default(),
        )
        .await
        .unwrap();

    assert_that!(&(added.version)).is_equal_to(item.version + 1);
    assert_that!(&(updated.version)).is_equal_to(added.version + 1);
    assert_that!(&(deleted.work_item.version)).is_equal_to(updated.version + 1);
    assert_that!(&(deleted.deleted)).is_true();
    assert_that!(&(deleted.label_id)).is_equal_to(label_id);
    assert_that!(&(deleted.work_item.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(
        &(!deleted
            .work_item
            .labels
            .iter()
            .any(|label| label.key == "priority"))
    )
    .is_true();

    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();
    let label_events: Vec<_> = events
        .iter()
        .filter(|event| {
            matches!(
                event.event_type,
                WorkItemEventType::LabelAdded
                    | WorkItemEventType::LabelUpdated
                    | WorkItemEventType::LabelDeleted
            )
        })
        .map(|event| (event.event_type, event.body.as_str()))
        .collect();
    assert_that!(&(label_events)).is_equal_to(vec![
        (WorkItemEventType::LabelAdded, "Added label priority=high"),
        (
            WorkItemEventType::LabelUpdated,
            "Updated label priority=low",
        ),
        (
            WorkItemEventType::LabelDeleted,
            "Deleted label priority=low",
        ),
    ]);
}

#[tokio::test]
async fn generic_label_mutations_reject_state_label_changes() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "State label item".to_owned(),
            description: "State changes must use the item move workflow".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let state_label_id = item
        .labels
        .iter()
        .find(|label| label.key == STATE_LABEL_KEY)
        .unwrap()
        .id;
    let priority = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "priority".to_owned(),
                value: Some("high".to_owned()),
            },
            Some(item.version),
            Default::default(),
        )
        .await
        .unwrap();
    let priority_label_id = priority
        .labels
        .iter()
        .find(|label| label.key == "priority")
        .unwrap()
        .id;

    let add_state = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: STATE_LABEL_KEY.to_owned(),
                value: Some("done".to_owned()),
            },
            Some(priority.version),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(add_state.to_string().contains("move the item"))).is_true();

    let update_state = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            item.id,
            state_label_id,
            dispatch_types::UpdateWorkItemLabelRequest {
                key: None,
                value: Some(Some("done".to_owned())),
                expect_version: Some(priority.version),
            },
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(update_state.to_string().contains("move the item"))).is_true();

    let rename_to_state = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            item.id,
            priority_label_id,
            dispatch_types::UpdateWorkItemLabelRequest {
                key: Some(STATE_LABEL_KEY.to_owned()),
                value: Some(Some("done".to_owned())),
                expect_version: Some(priority.version),
            },
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(rename_to_state.to_string().contains("move the item"))).is_true();

    let delete_state = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .delete(
            "demo",
            item.id,
            state_label_id,
            Some(priority.version),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(delete_state.to_string().contains("move the item"))).is_true();
}
pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::LabelService> {
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
    Arc::new(super::service::LabelService::new(
        transactions,
        projects,
        Arc::new(super::repository::LabelRepository),
        Arc::new(crate::backend::items::repository::ItemRepository),
        attribution,
        events,
    ))
}

#[tokio::test]
async fn history_failure_rolls_back_label_catalog_versions_and_notifications() {
    use dispatch_types::{CreateWorkItemLabelRequest, UpdateWorkItemLabelRequest};
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, item_id, _) = crate::backend::comments::tests::application().await;
    app.state
        .labels
        .add(
            "demo",
            item_id,
            CreateWorkItemLabelRequest {
                key: "priority".into(),
                value: Some("high".into()),
            },
            Some(1),
            Default::default(),
        )
        .await
        .unwrap();
    let before = app.state.items.get("demo", item_id).await.unwrap();
    let label_id = before
        .labels
        .iter()
        .find(|label| label.key == "priority")
        .unwrap()
        .id;
    let catalog = app.state.labels.project_labels("demo").await.unwrap();
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER fail_label_history BEFORE INSERT ON work_item_events BEGIN SELECT RAISE(ABORT, 'injected label history failure'); END".to_owned())).await.unwrap();
    assert_that!(
        &app.state
            .labels
            .add(
                "demo",
                item_id,
                CreateWorkItemLabelRequest {
                    key: "new-key".into(),
                    value: None
                },
                Some(before.version),
                Default::default()
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(
        &app.state
            .labels
            .update(
                "demo",
                item_id,
                label_id,
                UpdateWorkItemLabelRequest {
                    key: Some("renamed-key".into()),
                    value: Some(Some("low".into())),
                    expect_version: Some(before.version)
                },
                Default::default()
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(
        &app.state
            .labels
            .delete(
                "demo",
                item_id,
                label_id,
                Some(before.version),
                Default::default()
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.items.get("demo", item_id).await.unwrap()).is_equal_to(before);
    assert_that!(&app.state.labels.project_labels("demo").await.unwrap()).is_equal_to(catalog);
    assert_that!(&events.try_recv().is_err()).is_true();
}
