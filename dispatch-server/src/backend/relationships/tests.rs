use crate::backend::storage::Store;
use dispatch_types::*;

use assertr::prelude::*;
use tempfile::TempDir;

use crate::backend::{items::CreateWorkItem, projects::CreateProject};

async fn test_store() -> (TempDir, Store, i64, i64) {
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
    let source = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Source".to_owned(),
            description: "Creates the relationship".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let target = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Target".to_owned(),
            description: "Receives the relationship".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    (temp, store, source.id, target.id)
}

#[tokio::test]
async fn relationships_list_incoming_and_outgoing_entries() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, source_id, target_id) = test_store().await;
    crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            target_id,
            dispatch_types::UpdateWorkItemRequest {
                state: Some("review".to_owned()),
                expect_version: None,
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();

    let created = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            " is follow-up of ".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();

    assert_that!(&(created.direction)).is_equal_to(WorkItemRelationshipDirection::Outgoing);
    assert_that!(&(created.relationship.kind)).is_equal_to("is follow-up of");
    assert_that!(&(created.relationship.source.id)).is_equal_to(source_id);
    assert_that!(&(created.relationship.source.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(created.relationship.target.id)).is_equal_to(target_id);
    assert_that!(&(created.relationship.target.state.as_deref())).is_equal_to(Some("review"));

    let source_relationships = crate::backend::relationships::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .list("demo", source_id)
    .await
    .unwrap();
    assert_that!(&(source_relationships.len())).is_equal_to(1);
    assert_that!(&(source_relationships[0].direction))
        .is_equal_to(WorkItemRelationshipDirection::Outgoing);
    assert_that!(&(source_relationships[0].relationship.target.state.as_deref()))
        .is_equal_to(Some("review"));

    let target_relationships = crate::backend::relationships::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .list("demo", target_id)
    .await
    .unwrap();
    assert_that!(&(target_relationships.len())).is_equal_to(1);
    assert_that!(&(target_relationships[0].direction))
        .is_equal_to(WorkItemRelationshipDirection::Incoming);
}

#[tokio::test]
async fn relationship_create_validates_self_empty_duplicate_and_project_scope() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store, source_id, target_id) = test_store().await;

    let self_link = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            source_id,
            "duplicates".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(self_link.to_string().contains("must differ"))).is_true();

    let empty_kind = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            " ".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(empty_kind.to_string().contains("kind cannot be empty"))).is_true();

    crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "relates".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();
    let duplicate = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "relates".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(duplicate.to_string().contains("duplicate relationship"))).is_true();

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
    let other_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("other"),
        CreateWorkItem {
            title: "Other".to_owned(),
            description: "Other project".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let cross_project = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            other_item.id,
            "crosses".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(
        &(cross_project
            .to_string()
            .contains("does not exist in this project"))
    )
    .is_true();
}

#[tokio::test]
async fn relationship_update_and_delete_touch_both_items_and_emit_events() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, source_id, target_id) = test_store().await;
    let before_source =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", source_id)
            .await
            .unwrap();
    let before_target =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", target_id)
            .await
            .unwrap();
    let created = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "blocks".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();
    let relationship_id = created.relationship.id;

    let updated = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            None,
            relationship_id,
            "unblocks".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();
    assert_that!(&(updated.kind)).is_equal_to("unblocks");

    let after_update_source =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", source_id)
            .await
            .unwrap();
    let after_update_target =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", target_id)
            .await
            .unwrap();
    assert_that!(&(after_update_source.version)).is_equal_to(before_source.version + 2);
    assert_that!(&(after_update_target.version)).is_equal_to(before_target.version + 2);

    let deleted = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .delete("demo", None, relationship_id, Default::default())
        .await
        .unwrap();
    assert_that!(&(deleted.deleted)).is_true();
    assert_that!(
        &(crate::backend::relationships::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list("demo", source_id)
        .await
        .unwrap()
        .is_empty())
    )
    .is_true();
    assert_that!(
        &(crate::backend::relationships::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list("demo", target_id)
        .await
        .unwrap()
        .is_empty())
    )
    .is_true();

    let source_events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(source_id), None)
            .await
            .unwrap();
    let target_events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(target_id), None)
            .await
            .unwrap();
    for event_type in [
        WorkItemEventType::RelationshipCreated,
        WorkItemEventType::RelationshipUpdated,
        WorkItemEventType::RelationshipDeleted,
    ] {
        assert_that!(
            &(source_events
                .iter()
                .any(|event| event.event_type == event_type))
        )
        .with_detail_message(format!("missing source event {event_type}"))
        .is_true();
        assert_that!(
            &(target_events
                .iter()
                .any(|event| event.event_type == event_type))
        )
        .with_detail_message(format!("missing target event {event_type}"))
        .is_true();
    }
}

#[tokio::test]
async fn relationship_update_rejects_duplicate_kind() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, source_id, target_id) = test_store().await;
    crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "first".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();
    let second = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "second".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();

    let duplicate = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            None,
            second.relationship.id,
            "first".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();

    assert_that!(&(duplicate.to_string().contains("duplicate relationship"))).is_true();
}

#[tokio::test]
async fn item_scoped_relationship_mutations_require_requested_item_to_touch_relationship() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, source_id, target_id) = test_store().await;
    let unrelated = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Unrelated".to_owned(),
            description: "Does not touch the relationship".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let created = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "blocks".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();

    let update = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            Some(unrelated.id),
            created.relationship.id,
            "unblocks".to_owned(),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(update.to_string().contains("does not touch item"))).is_true();

    let delete = crate::backend::relationships::tests::service(&store, event_bus.clone())
        .delete(
            "demo",
            Some(unrelated.id),
            created.relationship.id,
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(&(delete.to_string().contains("does not touch item"))).is_true();

    let relationships = crate::backend::relationships::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .list("demo", source_id)
    .await
    .unwrap();
    assert_that!(&(relationships.len())).is_equal_to(1);
    assert_that!(&(relationships[0].relationship.kind)).is_equal_to("blocks");
}

#[tokio::test]
async fn deleting_work_item_cascades_relationships_without_orphans() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store, source_id, target_id) = test_store().await;
    crate::backend::relationships::tests::service(&store, event_bus.clone())
        .create(
            "demo",
            source_id,
            target_id,
            "blocks".to_owned(),
            Default::default(),
        )
        .await
        .unwrap();

    crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
        .delete(
            crate::backend::projects::ProjectReference::Name("demo"),
            source_id,
        )
        .await
        .unwrap();

    let target_relationships = crate::backend::relationships::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .list("demo", target_id)
    .await
    .unwrap();
    assert_that!(&(target_relationships.is_empty())).is_true();
}

pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::RelationshipService> {
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
    Arc::new(super::service::RelationshipService::new(
        transactions,
        projects,
        Arc::new(super::repository::RelationshipRepository),
        attribution,
        events,
    ))
}

#[tokio::test]
async fn endpoint_history_failure_rolls_back_every_relationship_mutation() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, source, target) = crate::backend::comments::tests::application().await;
    for operation in ["create", "update", "delete"] {
        let relationship = if operation == "create" {
            None
        } else {
            Some(
                app.state
                    .relationships
                    .create("demo", source, target, operation.into(), Default::default())
                    .await
                    .unwrap()
                    .relationship,
            )
        };
        let before = app.state.relationships.list("demo", source).await.unwrap();
        let source_before = crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", source)
        .await
        .unwrap();
        let target_before = crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", target)
        .await
        .unwrap();
        let history_before = crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", None, None)
        .await
        .unwrap();
        app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite, format!(
            "CREATE TRIGGER fail_target_history BEFORE INSERT ON work_item_events WHEN NEW.work_item_id = {target} BEGIN SELECT RAISE(ABORT, 'injected second endpoint failure'); END"))).await.unwrap();
        let mut events = app.state.events.subscribe();
        let failed = match relationship {
            None => app
                .state
                .relationships
                .create("demo", source, target, "blocks".into(), Default::default())
                .await
                .is_err(),
            Some(row) if operation == "update" => app
                .state
                .relationships
                .update("demo", None, row.id, "changed".into(), Default::default())
                .await
                .is_err(),
            Some(row) => app
                .state
                .relationships
                .delete("demo", None, row.id, Default::default())
                .await
                .is_err(),
        };
        assert_that!(&failed).is_true();
        assert_that!(&app.state.relationships.list("demo", source).await.unwrap())
            .is_equal_to(before);
        assert_that!(
            &crate::backend::items::tests::service(
                &app.state.store,
                crate::backend::events::UiEventBus::new()
            )
            .get("demo", source)
            .await
            .unwrap()
            .version
        )
        .is_equal_to(source_before.version);
        assert_that!(
            &crate::backend::items::tests::service(
                &app.state.store,
                crate::backend::events::UiEventBus::new()
            )
            .get("demo", target)
            .await
            .unwrap()
            .version
        )
        .is_equal_to(target_before.version);
        assert_that!(
            &serde_json::to_value(
                crate::backend::items::tests::service(
                    &app.state.store,
                    crate::backend::events::UiEventBus::new()
                )
                .events("demo", None, None)
                .await
                .unwrap()
            )
            .unwrap()
        )
        .is_equal_to(serde_json::to_value(history_before).unwrap());
        assert_that!(&events.try_recv().is_err()).is_true();
        app.state
            .store
            .db()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DROP TRIGGER fail_target_history".to_owned(),
            ))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn attributed_discussion_mutations_validate_runs_on_the_single_transaction_connection() {
    use crate::backend::{
        attribution::model::AttributionInput, entities::agent_run::AgentRunActiveModel,
        storage::utc_now,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use std::time::Duration;
    let (_temp, app, source, target) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let now = utc_now();
    let run = AgentRunActiveModel {
        project_id: Set(project_id),
        tool_name: Set("codex".into()),
        mutability: Set("read_only".into()),
        status: Set("running".into()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(app.state.store.db().as_ref())
    .await
    .unwrap();
    let input = AttributionInput {
        agent_id: Some(crate::backend::execution::identity::dispatch_run_agent_id(
            run.id,
        )),
        agent_run_id: Some(run.id),
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        let relationship = app
            .state
            .relationships
            .create("demo", source, target, "blocks".into(), input.clone())
            .await
            .unwrap()
            .relationship;
        app.state
            .relationships
            .update(
                "demo",
                Some(source),
                relationship.id,
                "follows".into(),
                input.clone(),
            )
            .await
            .unwrap();
        app.state
            .relationships
            .delete("demo", Some(target), relationship.id, input.clone())
            .await
            .unwrap();
        app.state
            .comments
            .add(
                crate::backend::comments::CommentTarget::ProjectItem {
                    project: "demo",
                    item_id: source,
                },
                AddCommentRequest {
                    author_type: AuthorType::Agent,
                    author_name: input.agent_id.clone(),
                    body: "Context".into(),
                },
                input.clone(),
            )
            .await
            .unwrap();
    })
    .await
    .expect("nested attribution and project scope must reuse the mutation connection");
    let history = crate::backend::items::tests::service(
        &app.state.store,
        crate::backend::events::UiEventBus::new(),
    )
    .events("demo", Some(source), None)
    .await
    .unwrap();
    assert_that!(
        &history
            .iter()
            .filter(|event| event.agent_run_id == Some(run.id))
            .count()
    )
    .is_equal_to(4);
    let mut events = app.state.events.subscribe();
    let wrong = AttributionInput {
        agent_id: Some("another-agent".into()),
        ..input.clone()
    };
    assert_that!(
        &app.state
            .relationships
            .create("demo", source, target, "blocks".into(), wrong.clone())
            .await
            .is_err()
    )
    .is_true();
    assert_that!(
        &app.state
            .comments
            .add(
                crate::backend::comments::CommentTarget::ProjectItem {
                    project: "demo",
                    item_id: source
                },
                AddCommentRequest {
                    author_type: AuthorType::Agent,
                    author_name: wrong.agent_id.clone(),
                    body: "Denied".into(),
                },
                wrong
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(
        &serde_json::to_value(
            crate::backend::items::tests::service(
                &app.state.store,
                crate::backend::events::UiEventBus::new()
            )
            .events("demo", Some(source), None)
            .await
            .unwrap()
        )
        .unwrap()
    )
    .is_equal_to(serde_json::to_value(history).unwrap());
    assert_that!(&events.try_recv().is_err()).is_true();
}
