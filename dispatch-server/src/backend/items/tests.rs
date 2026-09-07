use crate::backend::items::labels::repository::records as work_item_labels;
use crate::backend::{projects::repository::ProjectRepository, storage::Store};
use assertr::prelude::*;
use dispatch_types::AddCommentRequest;
use dispatch_types::UpdateWorkItemRequest;
use dispatch_types::*;
use sea_orm::{ConnectionTrait, Statement};
use tempfile::TempDir;

use super::*;
use crate::backend::projects::CreateProject;
use crate::shared::view_models::{AuthorType, CreateWorkItemLabelRequest};

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
    (temp, store)
}

#[tokio::test]
async fn work_items_are_scoped_to_project() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let demo_item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Demo item".to_owned(),
                description: "Build the demo item".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("other"),
            CreateWorkItem {
                title: "Other item".to_owned(),
                description: "Build the other item".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let demo_items =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap();
    let other_lookup =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("other", demo_item.id)
            .await
            .unwrap_err();

    assert_that!(&(demo_items.len())).is_equal_to(1);
    assert_that!(&(demo_items[0].title)).is_equal_to("Demo item");
    assert_that!(
        &(other_lookup
            .to_string()
            .contains("does not exist in this project"))
    )
    .is_true();
}

#[tokio::test]
async fn creating_item_records_item_created_event_row() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();

    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Record event".to_owned(),
                description: "Persist the creation event".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(events.len())).is_equal_to(1);
    assert_that!(&(events[0].project_id)).is_equal_to(project_id);
    assert_that!(&(events[0].work_item_id)).is_equal_to(Some(item.id));
    assert_that!(&(events[0].event_type)).is_equal_to(WorkItemEventType::ItemCreated);
    assert_that!(&(events[0].body)).is_equal_to("Created item");
    assert_that!(&(events[0].actor_type.is_none())).is_true();
    assert_that!(&(events[0].actor_id.is_none())).is_true();
    assert_that!(&(events[0].agent_run_id.is_none())).is_true();
}

#[tokio::test]
async fn creating_item_with_initial_labels_persists_normalized_labels() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Labeled create".to_owned(),
                description: "Initial labels should be visible immediately".to_owned(),
                state: " open ".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![
                    CreateWorkItemLabelRequest {
                        key: " type ".to_owned(),
                        value: Some(" feature ".to_owned()),
                    },
                    CreateWorkItemLabelRequest {
                        key: "needs-verification".to_owned(),
                        value: Some("  ".to_owned()),
                    },
                ],
            },
            Default::default(),
        )
        .await
        .unwrap();

    assert_that!(&(item.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(
        &(item.labels.iter().any(|label| {
            label.key == STATE_LABEL_KEY && label.value.as_deref() == Some("open")
        }))
    )
    .is_true();
    assert_that!(
        &(item
            .labels
            .iter()
            .any(|label| { label.key == "type" && label.value.as_deref() == Some("feature") }))
    )
    .is_true();
    assert_that!(
        &(item
            .labels
            .iter()
            .any(|label| label.key == "needs-verification" && label.value.is_none()))
    )
    .is_true();

    let listed =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap();
    let listed_item = listed
        .iter()
        .find(|candidate| candidate.id == item.id)
        .unwrap();
    assert_that!(
        &(listed_item
            .labels
            .iter()
            .any(|label| { label.key == "type" && label.value.as_deref() == Some("feature") }))
    )
    .is_true();
    assert_that!(
        &(listed_item
            .labels
            .iter()
            .any(|label| label.key == "needs-verification" && label.value.is_none()))
    )
    .is_true();
}

#[tokio::test]
async fn duplicate_initial_labels_reject_create_without_partial_item() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let err = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Duplicate labels".to_owned(),
                description: "Should not create anything".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![
                    CreateWorkItemLabelRequest {
                        key: " area ".to_owned(),
                        value: Some("frontend".to_owned()),
                    },
                    CreateWorkItemLabelRequest {
                        key: "area".to_owned(),
                        value: Some("backend".to_owned()),
                    },
                ],
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("duplicate initial label key"))).is_true();
    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .is_empty())
    )
    .is_true();
}

#[tokio::test]
async fn invalid_initial_label_key_rejects_create_without_partial_item() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let err = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Invalid label".to_owned(),
                description: "Should not create anything".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: "bad=key".to_owned(),
                    value: Some("value".to_owned()),
                }],
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("label key cannot contain '='"))).is_true();
    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .is_empty())
    )
    .is_true();
}

#[tokio::test]
async fn state_initial_label_rejects_create_without_partial_item() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let err = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "State collision".to_owned(),
                description: "State belongs to the selector".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: STATE_LABEL_KEY.to_owned(),
                    value: Some("review".to_owned()),
                }],
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("use the state selector"))).is_true();
    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .is_empty())
    )
    .is_true();
}

#[tokio::test]
async fn blank_state_selector_rejects_create_without_partial_item() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let err = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Blank state".to_owned(),
                description: "State cannot be blank".to_owned(),
                state: "  ".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: "type".to_owned(),
                    value: Some("feature".to_owned()),
                }],
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(
        &(err
            .to_string()
            .contains("state label value cannot be empty"))
    )
    .is_true();
    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .is_empty())
    )
    .is_true();
}

#[tokio::test]
async fn list_items_hydrates_labels_state_and_comment_counts() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let first = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "First item".to_owned(),
                description: "Has several comments and labels".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let second = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Second item".to_owned(),
                description: "Has an independent label and comment count".to_owned(),
                state: "ready".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            first.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "severity".to_owned(),
                value: Some("high".to_owned()),
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            second.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "bug".to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    for body in ["First comment", "Second comment"] {
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .add(
                crate::backend::comments::CommentTarget::ProjectItem {
                    project: "demo",
                    item_id: first.id,
                },
                AddCommentRequest {
                    author_type: AuthorType::User,
                    author_name: Some("operator".to_owned()),
                    body: body.to_owned(),
                },
                Default::default(),
            )
            .await
            .unwrap();
    }
    crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
        .add(
            crate::backend::comments::CommentTarget::ProjectItem {
                project: "demo",
                item_id: second.id,
            },
            AddCommentRequest {
                author_type: AuthorType::Agent,
                author_name: Some("agent-a".to_owned()),
                body: "Only comment".to_owned(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let items =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap();
    let first = items.iter().find(|item| item.id == first.id).unwrap();
    let second = items.iter().find(|item| item.id == second.id).unwrap();

    assert_that!(&(first.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(first.comment_count)).is_equal_to(2);
    assert_that!(
        &(first
            .labels
            .iter()
            .any(|label| { label.key == "severity" && label.value.as_deref() == Some("high") }))
    )
    .is_true();
    assert_that!(&(second.state.as_deref())).is_equal_to(Some("ready"));
    assert_that!(&(second.comment_count)).is_equal_to(1);
    assert_that!(
        &(second
            .labels
            .iter()
            .any(|label| { label.key == "bug" && label.value.is_none() }))
    )
    .is_true();
}

#[tokio::test]
async fn counts_items_outside_authored_work_item_states() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Valid item".to_owned(),
                description: "Uses an authored state".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Invalid item".to_owned(),
                description: "Uses an unconfigured state".to_owned(),
                state: "needs_triage".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let unlabeled = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Unlabeled item".to_owned(),
                description: "Has no state label".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("other"),
            CreateWorkItem {
                title: "Other project invalid item".to_owned(),
                description: "Should not affect the demo count".to_owned(),
                state: "needs_triage".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    work_item_labels::delete_by_key_in_tx(
        store.db().as_ref(),
        project_id,
        unlabeled.id,
        STATE_LABEL_KEY,
    )
    .await
    .unwrap();

    let demo_count =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .count_outside_states(ProjectRepository::new(store.db()).id("demo").await.unwrap())
            .await
            .unwrap();
    let other_count =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .count_outside_states(
                ProjectRepository::new(store.db())
                    .id("other")
                    .await
                    .unwrap(),
            )
            .await
            .unwrap();

    assert_that!(&(demo_count)).is_equal_to(2);
    assert_that!(&(other_count)).is_equal_to(1);
}

#[tokio::test]
async fn work_items_read_view_exposes_state_label() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Visible state".to_owned(),
                description: "Shows state in the CrudKit read view".to_owned(),
                state: "needs_triage".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();

    let row = store
        .db()
        .query_one(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            r#"
            SELECT "state_label"
            FROM "work_items_read_view"
            WHERE "project_id" = ?1
              AND "id" = ?2
            "#,
            vec![project_id.into(), item.id.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let state_label = row.try_get::<Option<String>>("", "state_label").unwrap();

    assert_that!(&(state_label.as_deref())).is_equal_to(Some("needs_triage"));
}

#[tokio::test]
async fn moving_item_updates_state_and_version() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Move me".to_owned(),
                description: "Move through states".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let moved = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            dispatch_types::UpdateWorkItemRequest {
                state: Some("in_progress".to_owned()),
                expect_version: Some(item.version),
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();

    assert_that!(&(moved.state.as_deref())).is_equal_to(Some("in_progress"));
    assert_that!(&(moved.version)).is_equal_to(item.version + 1);
}

#[tokio::test]
async fn updating_item_fields_and_state_is_one_versioned_change() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Update me".to_owned(),
                description: "Move and edit together".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let updated = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            UpdateWorkItemRequest {
                title: Some("Updated title".to_owned()),
                description: None,
                state: Some("review".to_owned()),
                agent_model_override: Some(Some("gpt-5.6-terra".to_owned())),
                agent_reasoning_effort_override: None,
                expect_version: Some(item.version),
            },
            Default::default(),
        )
        .await
        .unwrap();
    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(updated.title)).is_equal_to("Updated title");
    assert_that!(&(updated.state.as_deref())).is_equal_to(Some("review"));
    assert_that!(&(updated.agent_model_override.as_deref())).is_equal_to(Some("gpt-5.6-terra"));
    assert_that!(&(updated.version)).is_equal_to(item.version + 1);
    assert_that!(
        &(events
            .iter()
            .filter(|event| event.event_type == WorkItemEventType::ItemUpdated)
            .count())
    )
    .is_equal_to(1);
    assert_that!(
        &(events
            .iter()
            .filter(|event| event.event_type == WorkItemEventType::ItemMoved)
            .count())
    )
    .is_equal_to(1);
}

#[tokio::test]
async fn create_item_rejects_incompatible_effective_agent_reasoning() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let err = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Configured item".to_owned(),
                description: "Exercise incompatible reasoning".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: Some(
                    crate::shared::view_models::AgentReasoningEffort::Minimal,
                ),
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("effective agent model"))).is_true();
    assert_that!(&(err.to_string().contains("incompatible"))).is_true();
}

#[tokio::test]
async fn update_item_rejects_model_override_incompatible_with_inherited_reasoning() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Configured item".to_owned(),
                description: "Exercise inherited max reasoning".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let err = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            UpdateWorkItemRequest {
                agent_model_override: Some(Some("gpt-5.5".to_owned())),
                expect_version: Some(item.version),
                ..UpdateWorkItemRequest::default()
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("effective agent model"))).is_true();
    assert_that!(&(err.to_string().contains("max"))).is_true();
}

#[tokio::test]
async fn stale_expected_version_is_rejected() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Update me".to_owned(),
                description: "Expect conflict".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let err = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            UpdateWorkItemRequest {
                title: Some("Changed".to_owned()),
                description: None,
                state: None,
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                expect_version: Some(item.version + 1),
            },
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("version conflict"))).is_true();
}

#[tokio::test]
async fn empty_update_is_rejected_without_touching_item() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "No change".to_owned(),
                description: "Empty updates should not bump versions".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    let err = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            UpdateWorkItemRequest::default(),
            Default::default(),
        )
        .await
        .unwrap_err();
    let unchanged =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(err.to_string().contains("requires at least one field"))).is_true();
    assert_that!(&(unchanged.version)).is_equal_to(item.version);
    assert_that!(&(unchanged.updated_at)).is_equal_to(item.updated_at);
}

#[tokio::test]
async fn delete_removes_item_from_lists() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Delete me".to_owned(),
                description: "Hide after deletion".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();

    crate::backend::items::tests::service(&store, event_bus.clone())
        .delete(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
        )
        .await
        .unwrap();

    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .is_empty())
    )
    .is_true();
    assert_that!(
        &(crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .is_err())
    )
    .is_true();
}

pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::ItemService> {
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
    Arc::new(super::service::ItemService::new(
        transactions,
        projects,
        Arc::new(super::repository::ItemRepository),
        Arc::new(crate::backend::items::events::repository::EventRepository),
        attribution,
        events,
    ))
}
