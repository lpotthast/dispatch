use crate::backend::projects::repository::ProjectRepository;
use assertr::prelude::*;
use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use tempfile::TempDir;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use super::model::*;
use crate::backend::{
    entities::{agent_run, work_item::WorkItemActiveModel},
    execution::identity as agent_ids,
    items::CreateWorkItem,
    items::labels::repository::records as work_item_labels,
    projects::CreateProject,
    storage::{Store, utc_now},
};
use crate::shared::view_models::{
    AUTOMATION_BLOCKED_LABEL_KEY, AuthorType, CLAIMED_FROM_STATE_LABEL_KEY, CLAIMED_STATE_LABEL,
    FEEDBACK_REQUESTED_LABEL_KEY, FINISHED_STATE_LABEL, STATE_LABEL_KEY, WorkItemEventType,
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

fn open_state_selector() -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String("open".to_owned()),
    })])
}

async fn seed_claim_source_label(store: &Store, project_name: &str, item_id: i64, state: &str) {
    let project_id = ProjectRepository::new(store.db())
        .id(project_name)
        .await
        .unwrap();
    work_item_labels::upsert_in_tx(
        store.db().as_ref(),
        project_id,
        item_id,
        CLAIMED_FROM_STATE_LABEL_KEY,
        Some(state),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn claiming_item_records_agent_identity() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Claim me".to_owned(),
            description: "Available work".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    let comments =
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", item.id)
            .await
            .unwrap();
    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(claimed.id)).is_equal_to(item.id);
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some("in_progress"));
    assert_that!(&(claimed.version)).is_equal_to(item.version + 1);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
    assert_that!(&(claimed.claimed_at.is_some())).is_true();
    assert_that!(
        &(comments
            .iter()
            .any(|comment| comment.body == "Claimed by agent-a"))
    )
    .is_true();
    assert_that!(
        &(events
            .iter()
            .any(|event| event.event_type == WorkItemEventType::ItemClaimed))
    )
    .is_true();
}

#[tokio::test]
async fn progress_records_agent_comment_and_touches_item() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Progress target".to_owned(),
            description: "Progress should update visible item metadata".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    let comment = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .progress_item("demo", item.id, "agent-a", "Working", Default::default())
    .await
    .unwrap();
    let updated =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();
    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(comment.author_type)).is_equal_to(AuthorType::Agent);
    assert_that!(&(comment.author_name.as_deref())).is_equal_to(Some("agent-a"));
    assert_that!(&(comment.body)).is_equal_to("Working");
    assert_that!(&(updated.version)).is_equal_to(claimed.version + 1);
    assert_that!(&(updated.comment_count)).is_equal_to(2);
    assert_that!(
        &(events.iter().any(|event| {
            event.event_type == WorkItemEventType::ProgressAdded && event.body == "Working"
        }))
    )
    .is_true();
}

#[tokio::test]
async fn claiming_can_use_nested_label_conditions() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Plain item".to_owned(),
            description: "Should not match the selector".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let matching = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Urgent bug".to_owned(),
            description: "Should match the selector".to_owned(),
            state: "open".to_owned(),
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
            matching.id,
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
            matching.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "bug".to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    let selector = Condition::All(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: "state".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("open".to_owned()),
        }),
        ConditionElement::Condition(Box::new(Condition::Any(vec![
            ConditionElement::Clause(ConditionClause {
                column_name: "severity".to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::String("high".to_owned()),
            }),
            ConditionElement::Clause(ConditionClause {
                column_name: "bug".to_owned(),
                operator: Operator::Equal,
                value: ConditionClauseValue::Bool(true),
            }),
        ]))),
    ]);

    assert_that!(
        &(crate::backend::items::claims::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .has_claimable_item_matching_condition("demo", &selector)
        .await
        .unwrap())
    )
    .is_true();

    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_item_matching_condition("demo", "agent-a", &selector)
        .await
        .unwrap()
        .unwrap();

    assert_that!(&(claimed.id)).is_equal_to(matching.id);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some("in_progress"));
}

#[tokio::test]
async fn blocked_items_are_skipped_by_selector_claims() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Blocked bug".to_owned(),
            description: "Should wait for human triage".to_owned(),
            state: "open".to_owned(),
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
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: "severity".to_owned(),
                value: Some("high".to_owned()),
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    let selector = Condition::All(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: "state".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("open".to_owned()),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: "severity".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("high".to_owned()),
        }),
    ]);

    assert_that!(
        &(!crate::backend::items::claims::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .has_claimable_item_matching_condition("demo", &selector)
        .await
        .unwrap())
    )
    .is_true();
    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_item_matching_condition("demo", "agent-a", &selector)
        .await
        .unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn claimed_from_state_label_is_private_workflow_bookkeeping() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Private claim source".to_owned(),
            description: "Release must trust only workflow-owned claim source labels".to_owned(),
            state: "review".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "review", Default::default())
    .await
    .unwrap()
    .unwrap();
    let claimed_from_label_id = claimed
        .labels
        .iter()
        .find(|label| label.key == CLAIMED_FROM_STATE_LABEL_KEY)
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
            Some(claimed.version),
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

    let add_claim_source = crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: CLAIMED_FROM_STATE_LABEL_KEY.to_owned(),
                value: Some("open".to_owned()),
            },
            Some(priority.version),
            Default::default(),
        )
        .await
        .unwrap_err();
    assert_that!(
        &(add_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let update_claim_source =
        crate::backend::items::labels::tests::service(&store, event_bus.clone())
            .update(
                "demo",
                item.id,
                claimed_from_label_id,
                dispatch_types::UpdateWorkItemLabelRequest {
                    key: None,
                    value: Some(Some("open".to_owned())),
                    expect_version: Some(priority.version),
                },
                Default::default(),
            )
            .await
            .unwrap_err();
    assert_that!(
        &(update_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let rename_to_claim_source =
        crate::backend::items::labels::tests::service(&store, event_bus.clone())
            .update(
                "demo",
                item.id,
                priority_label_id,
                dispatch_types::UpdateWorkItemLabelRequest {
                    key: Some(CLAIMED_FROM_STATE_LABEL_KEY.to_owned()),
                    value: Some(Some("open".to_owned())),
                    expect_version: Some(priority.version),
                },
                Default::default(),
            )
            .await
            .unwrap_err();
    assert_that!(
        &(rename_to_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let delete_claim_source =
        crate::backend::items::labels::tests::service(&store, event_bus.clone())
            .delete(
                "demo",
                item.id,
                claimed_from_label_id,
                Some(priority.version),
                Default::default(),
            )
            .await
            .unwrap_err();
    assert_that!(
        &(delete_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        item.id,
        "agent-a",
        Some("done for now".to_owned()),
        ReleaseAutomationDisposition::Blocked,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("review"));
}

#[tokio::test]
async fn specific_selector_claims_skip_workflow_blockers() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let selector = open_state_selector();

    for key in [AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY] {
        let item = crate::backend::items::creation::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: format!("Blocked by {key}"),
                description: "Specific automation claims should still honor blockers".to_owned(),
                state: "open".to_owned(),
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
                item.id,
                crate::shared::view_models::CreateWorkItemLabelRequest {
                    key: key.to_owned(),
                    value: None,
                },
                None,
                Default::default(),
            )
            .await
            .unwrap();

        assert_that!(
            &(!crate::backend::items::claims::tests::service(
                &store,
                crate::backend::events::UiEventBus::new()
            )
            .has_claimable_specific_item_matching_condition("demo", item.id, &selector)
            .await
            .unwrap())
        )
        .is_true();
        let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
            .claim_specific_item_matching_condition("demo", item.id, "agent-a", &selector)
            .await
            .unwrap();
        let reloaded = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", item.id)
        .await
        .unwrap();

        assert_that!(&(claimed.is_none())).is_true();
        assert_that!(&(reloaded.claimed_by)).is_equal_to(None);
        assert_that!(&(reloaded.state.as_deref())).is_equal_to(Some("open"));
    }
}

#[tokio::test]
async fn claiming_is_atomic_for_racing_agents() {
    let (_temp, store) = test_store().await;
    crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Race item".to_owned(),
            description: "Only one agent can own this".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let first_service = service(&store, crate::backend::events::UiEventBus::new());
    let second_service = service(&store, crate::backend::events::UiEventBus::new());
    let (first, second) = tokio::join!(
        first_service.claim_item("demo", "agent-a", "open", Default::default()),
        second_service.claim_item("demo", "agent-b", "open", Default::default())
    );
    let claims = [first.unwrap(), second.unwrap()];
    let in_progress =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", Some("in_progress".to_owned()))
            .await
            .unwrap();

    assert_that!(&(claims.iter().filter(|claim| claim.is_some()).count())).is_equal_to(1);
    assert_that!(&(in_progress.len())).is_equal_to(1);
    assert_that!(
        &(matches!(
            in_progress[0].claimed_by.as_deref(),
            Some("agent-a" | "agent-b")
        ))
    )
    .is_true();
}

#[tokio::test]
async fn claim_respects_project_scope() {
    let (_temp, store) = test_store().await;
    crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("other"),
        CreateWorkItem {
            title: "Other item".to_owned(),
            description: "Should not be claimed from demo".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn idea_item_is_skipped_until_moved_open() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Draft item".to_owned(),
            description: "Hold this back from automation".to_owned(),
            state: "idea".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let skipped = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap();
    assert_that!(&(skipped.is_none())).is_true();

    let opened = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            dispatch_types::UpdateWorkItemRequest {
                state: Some("open".to_owned()),
                expect_version: Some(item.version),
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();
    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    assert_that!(&(opened.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(claimed.id)).is_equal_to(item.id);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
}

#[tokio::test]
async fn claiming_scans_past_non_matching_candidate_batch() {
    let (_temp, store) = test_store().await;
    for title in ["Draft one", "Draft two"] {
        crate::backend::items::creation::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: title.to_owned(),
                description: "This item should not match an open-state claim".to_owned(),
                state: "idea".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
            Default::default(),
        )
        .await
        .unwrap();
    }
    let open = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Open after drafts".to_owned(),
            description: "The scanner should continue until it finds this item".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    assert_that!(&(claimed.id)).is_equal_to(open.id);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
}

#[tokio::test]
async fn claimed_items_include_verified_automation_source() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Refine me".to_owned(),
            description: "A trigger should be visible while claimed".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let now = utc_now();
    let run = agent_run::ActiveModel {
        project_id: Set(item.project_id),
        work_item_id: Set(Some(item.id)),
        memory_event_id: Set(None),
        trigger_id: Set(Some(7)),
        trigger_name: Set(Some("Refine queued item".to_owned())),
        tool_name: Set("codex".to_owned()),
        mutability: Set("read_only".to_owned()),
        status: Set("running".to_owned()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        worktree_path: Set(None),
        branch_name: Set(None),
        process_id: Set(None),
        exit_code: Set(None),
        log_path: Set(None),
        developer_instructions_path: Set(None),
        user_prompt_path: Set(None),
        agent_model: Set(None),
        agent_reasoning_effort: Set(None),
        input_tokens: Set(None),
        cached_input_tokens: Set(None),
        output_tokens: Set(None),
        commit_required: Set(false),
        commit_outcome: Set("not_evaluated".to_owned()),
        commit_shas: Set("[]".to_owned()),
        pr_requested: Set(false),
        pr_url: Set(None),
        cleanup_status: Set("not_applicable".to_owned()),
        worktree_cleaned_at: Set(None),
        result_summary: Set(String::new()),
        started_at: Set(Some(now.clone())),
        finished_at: Set(None),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .unwrap();
    let agent_id = agent_ids::dispatch_run_agent_id(run.id);

    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, &agent_id)
        .await
        .unwrap()
        .unwrap();
    let item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();
    let listed =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", None)
            .await
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == item.id)
            .unwrap();

    for view in [item, listed] {
        let claim_source = view.claim_source.expect("claim source should be present");
        assert_that!(&(claim_source.run_id)).is_equal_to(run.id);
        assert_that!(&(claim_source.trigger_id)).is_equal_to(Some(7));
        assert_that!(&(claim_source.trigger_name.as_deref()))
            .is_equal_to(Some("Refine queued item"));
    }
}

#[tokio::test]
async fn claimed_items_ignore_unlinked_dispatch_run_claimants() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Claim me".to_owned(),
            description: "Source should not be guessed from a mismatched run".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let other = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Other".to_owned(),
            description: "The run is structurally linked here instead".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let now = utc_now();
    let run = agent_run::ActiveModel {
        project_id: Set(item.project_id),
        work_item_id: Set(Some(other.id)),
        memory_event_id: Set(None),
        trigger_id: Set(Some(8)),
        trigger_name: Set(Some("Wrong source".to_owned())),
        tool_name: Set("codex".to_owned()),
        mutability: Set("mutating".to_owned()),
        status: Set("running".to_owned()),
        command: Set(String::new()),
        working_dir: Set(String::new()),
        worktree_path: Set(None),
        branch_name: Set(None),
        process_id: Set(None),
        exit_code: Set(None),
        log_path: Set(None),
        developer_instructions_path: Set(None),
        user_prompt_path: Set(None),
        agent_model: Set(None),
        agent_reasoning_effort: Set(None),
        input_tokens: Set(None),
        cached_input_tokens: Set(None),
        output_tokens: Set(None),
        commit_required: Set(false),
        commit_outcome: Set("not_evaluated".to_owned()),
        commit_shas: Set("[]".to_owned()),
        pr_requested: Set(false),
        pr_url: Set(None),
        cleanup_status: Set("not_applicable".to_owned()),
        worktree_cleaned_at: Set(None),
        result_summary: Set(String::new()),
        started_at: Set(Some(now.clone())),
        finished_at: Set(None),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .unwrap();
    let agent_id = agent_ids::dispatch_run_agent_id(run.id);

    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, &agent_id)
        .await
        .unwrap()
        .unwrap();
    let item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(item.claimed_by.as_deref())).is_equal_to(Some(agent_id.as_str()));
    assert_that!(&(item.claim_source.is_none())).is_true();
}

#[tokio::test]
async fn release_restores_claim_source_state_and_blocks_automation() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Custom lane item".to_owned(),
            description: "Release should return to this lane".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "ready", Default::default())
    .await
    .unwrap()
    .unwrap();
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some(CLAIMED_STATE_LABEL));
    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("ready")
        }))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        item.id,
        "agent-a",
        Some("Cannot operate on this item.".to_owned()),
        ReleaseAutomationDisposition::Blocked,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("ready"));
    assert_that!(&(released.claimed_by)).is_equal_to(None);
    assert_that!(
        &(released
            .labels
            .iter()
            .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
    assert_that!(
        &(released
            .labels
            .iter()
            .all(|label| label.key != CLAIMED_FROM_STATE_LABEL_KEY))
    )
    .is_true();

    let claimed_again = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "ready", Default::default())
    .await
    .unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn claimable_release_clears_existing_automation_blocker() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Manual retry".to_owned(),
            description: "A direct retry should be able to reopen automation.".to_owned(),
            state: "open".to_owned(),
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
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, "agent-a")
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some(CLAIMED_STATE_LABEL));
    assert_that!(
        &(claimed
            .labels
            .iter()
            .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(released.claimed_by)).is_equal_to(None);
    for key in [
        CLAIMED_FROM_STATE_LABEL_KEY,
        AUTOMATION_BLOCKED_LABEL_KEY,
        FEEDBACK_REQUESTED_LABEL_KEY,
    ] {
        assert_that!(&(released.labels.iter().all(|label| label.key != key)))
            .with_detail_message(format!("claimable release should remove {key}"))
            .is_true();
    }

    let claimed_again = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    assert_that!(&(claimed_again.id)).is_equal_to(item.id);
    assert_that!(&(claimed_again.claimed_by.as_deref())).is_equal_to(Some("agent-b"));
}

#[tokio::test]
async fn failed_automation_claim_finalization_blocks_retry() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Failed setup".to_owned(),
            description: "Failed automation should not re-enter claim selection immediately."
                .to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-17";
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", agent_id, "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .finalize_automation_claim(AutomationClaimFinalization {
            project_id: item.project_id,
            project_name: "demo",
            run_id: 17,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("Workspace setup failed\nbecause git was unavailable."),
        })
        .await
        .unwrap();

    let released =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();
    let comments =
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(released.claimed_by)).is_equal_to(None);
    assert_that!(
        &(released
            .labels
            .iter()
            .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
    assert_that!(
        &(comments.iter().any(|comment| {
            comment.author_type == AuthorType::Agent
                && comment.author_name.as_deref() == Some(agent_id)
                && comment.body.contains("Automation turn failed")
                && comment.body.contains("Run #17")
                && comment
                    .body
                    .contains("Workspace setup failed because git was unavailable.")
        }))
    )
    .is_true();

    let claimed_again = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "open", Default::default())
    .await
    .unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn successful_and_cancelled_automation_claim_finalization_remains_claimable() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    for (index, outcome) in [
        (1, AutomationClaimOutcome::CompletedUnfinished),
        (2, AutomationClaimOutcome::Cancelled),
    ] {
        let source_state = format!("ready-{index}");
        let item = crate::backend::items::creation::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(
            crate::backend::projects::ProjectReference::Name("demo"),
            CreateWorkItem {
                title: format!("Claimable outcome {index}"),
                description: "Non-failed automation outcomes should release without blockers."
                    .to_owned(),
                state: source_state.clone(),
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
                item.id,
                crate::shared::view_models::CreateWorkItemLabelRequest {
                    key: AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
                    value: None,
                },
                None,
                Default::default(),
            )
            .await
            .unwrap();
        let agent_id = format!("dispatch-run-{}", 20 + index);
        crate::backend::items::claims::tests::service(&store, event_bus.clone())
            .claim_specific_item("demo", item.id, &agent_id)
            .await
            .unwrap()
            .unwrap();

        crate::backend::items::claims::tests::service(&store, event_bus.clone())
            .finalize_automation_claim(AutomationClaimFinalization {
                project_id: item.project_id,
                project_name: "demo",
                run_id: 20 + index,
                claimed_item_id: Some(item.id),
                agent_id: &agent_id,
                outcome,
                detail: None,
            })
            .await
            .unwrap();

        let released = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", item.id)
        .await
        .unwrap();
        assert_that!(&(released.state.as_deref())).is_equal_to(Some(source_state.as_str()));
        assert_that!(&(released.claimed_by)).is_equal_to(None);
        assert_that!(
            &(released
                .labels
                .iter()
                .all(|label| label.key != AUTOMATION_BLOCKED_LABEL_KEY))
        )
        .is_true();

        let claimed_again = crate::backend::items::claims::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .claim_item("demo", "agent-b", &source_state, Default::default())
        .await
        .unwrap()
        .unwrap();
        assert_that!(&(claimed_again.id)).is_equal_to(item.id);
        crate::backend::items::claims::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .release_item(
            "demo",
            item.id,
            "agent-b",
            None,
            ReleaseAutomationDisposition::Claimable,
            Default::default(),
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn automation_claim_finalization_uses_finish_metadata_not_state_label() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Moved to done".to_owned(),
            description: "A done state label alone should not suppress claim cleanup.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-31";
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", agent_id, "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            dispatch_types::UpdateWorkItemRequest {
                state: Some(FINISHED_STATE_LABEL.to_owned()),
                expect_version: None,
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();

    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .finalize_automation_claim(AutomationClaimFinalization {
            project_id: item.project_id,
            project_name: "demo",
            run_id: 31,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("Run failed after the state label changed."),
        })
        .await
        .unwrap();

    let released =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();
    assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(released.claimed_by)).is_equal_to(None);
    assert_that!(&(released.finished_at.is_none())).is_true();
    assert_that!(
        &(released
            .labels
            .iter()
            .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
}

#[tokio::test]
async fn automation_claim_finalization_leaves_finished_items_alone() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Already finished".to_owned(),
            description: "Finished timestamp is the terminal item marker.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-41";
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", agent_id, "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    let model = crate::backend::items::repository::records::get(
        store.db().as_ref(),
        item.project_id,
        item.id,
    )
    .await
    .unwrap();
    let mut active: WorkItemActiveModel = model.into();
    active.finished_at = Set(Some(utc_now()));
    active.update(store.db().as_ref()).await.unwrap();

    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .finalize_automation_claim(AutomationClaimFinalization {
            project_id: item.project_id,
            project_name: "demo",
            run_id: 41,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("This should be ignored."),
        })
        .await
        .unwrap();

    let current =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();
    assert_that!(&(current.claimed_by.as_deref())).is_equal_to(Some(agent_id));
    assert_that!(&(current.finished_at.is_some())).is_true();
    assert_that!(
        &(current
            .labels
            .iter()
            .all(|label| label.key != AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
}

#[tokio::test]
async fn request_feedback_restores_source_state_and_blocks_automation() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Needs input".to_owned(),
            description: "Agent should ask for a user decision".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "ready", Default::default())
    .await
    .unwrap()
    .unwrap();

    let updated = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .request_feedback(
            "demo",
            item.id,
            "agent-a",
            "Which provider should this integration target?",
            Default::default(),
        )
        .await
        .unwrap();
    let comments =
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", item.id)
            .await
            .unwrap();
    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(updated.state.as_deref())).is_equal_to(Some("ready"));
    assert_that!(&(updated.claimed_by)).is_equal_to(None);
    assert_that!(
        &(updated
            .labels
            .iter()
            .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
    assert_that!(
        &(updated
            .labels
            .iter()
            .any(|label| label.key == FEEDBACK_REQUESTED_LABEL_KEY))
    )
    .is_true();
    assert_that!(
        &(updated
            .labels
            .iter()
            .all(|label| label.key != CLAIMED_FROM_STATE_LABEL_KEY))
    )
    .is_true();
    assert_that!(
        &(comments.iter().any(|comment| {
            comment.author_type == AuthorType::Agent
                && comment.author_name.as_deref() == Some("agent-a")
                && comment.body == "Which provider should this integration target?"
        }))
    )
    .is_true();
    assert_that!(
        &(events
            .iter()
            .any(|event| event.event_type == WorkItemEventType::FeedbackRequested))
    )
    .is_true();

    let claimed_again = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "ready", Default::default())
    .await
    .unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn feedback_requested_label_blocks_state_claims() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Awaiting answer".to_owned(),
            description: "Feedback label alone should block automation pickup".to_owned(),
            state: "open".to_owned(),
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
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn specific_claim_release_restores_current_state() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Manual retry".to_owned(),
            description: "Explicit item claims are not tied to open".to_owned(),
            state: "triage".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, "agent-a")
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some(CLAIMED_STATE_LABEL));

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("triage"));
    assert_that!(&(released.claimed_by)).is_equal_to(None);
    assert_that!(
        &(released
            .labels
            .iter()
            .all(|label| label.key != AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();

    let claimed_again = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "triage", Default::default())
    .await
    .unwrap()
    .unwrap();
    assert_that!(&(claimed_again.id)).is_equal_to(item.id);
    assert_that!(&(claimed_again.claimed_by.as_deref())).is_equal_to(Some("agent-b"));
}

#[tokio::test]
async fn new_claims_overwrite_stale_claim_source_with_current_state() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let state_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "State retry".to_owned(),
            description: "State claims use the current state as release source".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", state_item.id, "ready").await;

    let claimed = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-state", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("open")
        }))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        state_item.id,
        "agent-state",
        None,
        ReleaseAutomationDisposition::Claimable,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));

    let selector_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Selector retry".to_owned(),
            description: "Claim source should come from the current state label".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", selector_item.id, "open").await;

    let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String("ready".to_owned()),
    })]);
    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_item_matching_condition("demo", "agent-a", &selector)
        .await
        .unwrap()
        .unwrap();

    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some(CLAIMED_STATE_LABEL));
    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("ready")
        }))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        selector_item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("ready"));

    let specific_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Specific retry".to_owned(),
            description: "Specific claims use the same source-state rule".to_owned(),
            state: "triage".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", specific_item.id, "open").await;

    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", specific_item.id, "agent-b")
        .await
        .unwrap()
        .unwrap();

    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("triage")
        }))
    )
    .is_true();

    let released = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        specific_item.id,
        "agent-b",
        None,
        ReleaseAutomationDisposition::Claimable,
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("triage"));
}

#[tokio::test]
async fn release_requires_current_claimant() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Owned item".to_owned(),
            description: "Only the claimant can release it".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    let err = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .release_item(
        "demo",
        item.id,
        "agent-b",
        None,
        ReleaseAutomationDisposition::Blocked,
        Default::default(),
    )
    .await
    .unwrap_err();

    assert_that!(&(err.to_string().contains("claimed by agent-a"))).is_true();
}

#[tokio::test]
async fn finish_clears_claim_and_blocking_workflow_labels() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Finish blocked item".to_owned(),
            description: "Completion should clear workflow bookkeeping labels".to_owned(),
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
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();
    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, "agent-a")
        .await
        .unwrap()
        .unwrap();
    crate::backend::items::labels::tests::service(&store, event_bus.clone())
        .add(
            "demo",
            item.id,
            crate::shared::view_models::CreateWorkItemLabelRequest {
                key: FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
                value: None,
            },
            None,
            Default::default(),
        )
        .await
        .unwrap();

    let finished = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .finish_item(
        "demo",
        item.id,
        "agent-a",
        "Finished cleanly",
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(&(finished.state.as_deref())).is_equal_to(Some(FINISHED_STATE_LABEL));
    assert_that!(&(finished.claimed_by)).is_equal_to(None);
    assert_that!(&(finished.finished_at.is_some())).is_true();
    for key in [
        CLAIMED_FROM_STATE_LABEL_KEY,
        AUTOMATION_BLOCKED_LABEL_KEY,
        FEEDBACK_REQUESTED_LABEL_KEY,
    ] {
        assert_that!(&(finished.labels.iter().all(|label| label.key != key)))
            .with_detail_message(format!("finished item should not retain {key}"))
            .is_true();
    }
}

#[tokio::test]
async fn finish_moves_done_and_records_report() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Finish item".to_owned(),
            description: "Complete with report".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();

    let finished = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .finish_item(
        "demo",
        item.id,
        "agent-a",
        "Finished cleanly",
        Default::default(),
    )
    .await
    .unwrap();
    let comments =
        crate::backend::comments::tests::service(&store, crate::backend::events::UiEventBus::new())
            .list("demo", item.id)
            .await
            .unwrap();
    let events =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap();

    assert_that!(&(finished.state.as_deref())).is_equal_to(Some("done"));
    assert_that!(&(finished.claimed_by)).is_equal_to(None);
    assert_that!(&(finished.finished_at.is_some())).is_true();
    assert_that!(
        &(comments
            .iter()
            .any(|comment| comment.body == "Finished cleanly"))
    )
    .is_true();
    assert_that!(
        &(events
            .iter()
            .any(|event| event.event_type == WorkItemEventType::ItemFinished))
    )
    .is_true();
}

#[tokio::test]
async fn state_and_selector_claims_do_not_reopen_finished_items() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Finished item".to_owned(),
            description: "State changes alone should not reopen finished work".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    let finished = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .finish_item("demo", item.id, "agent-a", "Finished", Default::default())
    .await
    .unwrap();
    let moved = crate::backend::items::tests::service(&store, event_bus.clone())
        .update(
            crate::backend::projects::ProjectReference::Name("demo"),
            item.id,
            dispatch_types::UpdateWorkItemRequest {
                state: Some("open".to_owned()),
                expect_version: Some(finished.version),
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();
    let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String("open".to_owned()),
    })]);

    let state_claim = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-b", "open", Default::default())
    .await
    .unwrap();
    let selector_has_match = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .has_claimable_item_matching_condition("demo", &selector)
    .await
    .unwrap();
    let selector_claim = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_item_matching_condition("demo", "agent-c", &selector)
        .await
        .unwrap();
    let reloaded =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(moved.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(moved.finished_at.is_some())).is_true();
    assert_that!(&(state_claim.is_none())).is_true();
    assert_that!(&(!selector_has_match)).is_true();
    assert_that!(&(selector_claim.is_none())).is_true();
    assert_that!(&(reloaded.claimed_by)).is_equal_to(None);
    assert_that!(&(reloaded.finished_at.is_some())).is_true();
}

#[tokio::test]
async fn specific_claim_does_not_reopen_finished_items() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Finished item".to_owned(),
            description: "Should stay closed after completion".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .finish_item("demo", item.id, "agent-a", "Finished", Default::default())
    .await
    .unwrap();

    let claimed = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .claim_specific_item("demo", item.id, "agent-b")
        .await
        .unwrap();
    let reloaded =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(claimed.is_none())).is_true();
    assert_that!(&(reloaded.state.as_deref())).is_equal_to(Some(FINISHED_STATE_LABEL));
    assert_that!(&(reloaded.claimed_by)).is_equal_to(None);
    assert_that!(&(reloaded.finished_at.is_some())).is_true();
}

#[tokio::test]
async fn stale_claim_recovery_releases_old_claim() {
    let (_temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Stale item".to_owned(),
            description: "Claim should be recovered".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .claim_item("demo", "agent-a", "open", Default::default())
    .await
    .unwrap()
    .unwrap();
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    let mut model: WorkItemActiveModel =
        crate::backend::items::repository::records::get(store.db().as_ref(), project_id, item.id)
            .await
            .unwrap()
            .into();
    model.claimed_at = Set(Some(
        (OffsetDateTime::now_utc() - Duration::minutes(30))
            .format(&Rfc3339)
            .unwrap(),
    ));
    model.update(store.db().as_ref()).await.unwrap();

    let recovered = crate::backend::items::claims::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .recover_stale_claims("demo", 10)
    .await
    .unwrap();
    let item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(recovered.len())).is_equal_to(1);
    assert_that!(&(recovered[0].agent_id)).is_equal_to("agent-a");
    assert_that!(&(item.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(item.claimed_by)).is_equal_to(None);
}

pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::ClaimService> {
    use std::sync::Arc;
    let transactions = Arc::new(crate::backend::storage::TransactionManager::new(store));
    let projects = Arc::new(ProjectRepository::new(store.db()));
    let attribution = Arc::new(
        crate::backend::attribution::service::AttributionService::new(
            transactions.clone(),
            projects.clone(),
            Arc::new(crate::backend::attribution::repository::AttributionRepository::new()),
        ),
    );
    Arc::new(super::service::ClaimService::new(
        transactions,
        projects,
        Arc::new(super::repository::ClaimRepository),
        Arc::new(crate::backend::items::repository::ItemRepository),
        Arc::new(crate::backend::runs::launch::repository::AgentRunLaunchRepository),
        attribution,
        events,
    ))
}

#[tokio::test]
async fn claim_workflow_history_failures_roll_back_state_comments_and_notifications() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, item_id, _) = crate::backend::comments::tests::application().await;
    let db = app.state.store.db();
    let mut events = app.state.events.subscribe();
    let fail = "CREATE TRIGGER reject_claim_history BEFORE INSERT ON work_item_events BEGIN SELECT RAISE(ABORT,'injected claim history failure'); END";
    db.execute(Statement::from_string(DbBackend::Sqlite, fail.to_owned()))
        .await
        .unwrap();
    let before = app.state.items.get("demo", item_id).await.unwrap();
    assert_that!(
        &app.state
            .claims
            .claim_item("demo", "agent-test", "open", Default::default())
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.items.get("demo", item_id).await.unwrap()).is_equal_to(before.clone());
    assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()).is_empty();
    assert_that!(&events.try_recv().is_err()).is_true();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "DROP TRIGGER reject_claim_history".to_owned(),
    ))
    .await
    .unwrap();
    let claimed = app
        .state
        .claims
        .claim_item("demo", "agent-test", "open", Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_that!(&claimed.id).is_equal_to(item_id);
    events.try_recv().unwrap();
    let comments = app.state.comments.list("demo", item_id).await.unwrap();
    let history = serde_json::to_value(
        app.state
            .items
            .events("demo", Some(item_id), None)
            .await
            .unwrap(),
    )
    .unwrap();
    db.execute(Statement::from_string(DbBackend::Sqlite, fail.to_owned()))
        .await
        .unwrap();
    for operation in 0..5 {
        let failed = match operation {
            0 => app
                .state
                .claims
                .progress_item(
                    "demo",
                    item_id,
                    "agent-test",
                    "Progress",
                    Default::default(),
                )
                .await
                .is_err(),
            1 => app
                .state
                .claims
                .finish_item(
                    "demo",
                    item_id,
                    "agent-test",
                    "Finished",
                    Default::default(),
                )
                .await
                .is_err(),
            2 => app
                .state
                .claims
                .release_item(
                    "demo",
                    item_id,
                    "agent-test",
                    Some("Release".into()),
                    ReleaseAutomationDisposition::Blocked,
                    Default::default(),
                )
                .await
                .is_err(),
            3 => app
                .state
                .claims
                .request_feedback(
                    "demo",
                    item_id,
                    "agent-test",
                    "Feedback",
                    Default::default(),
                )
                .await
                .is_err(),
            _ => app
                .state
                .claims
                .finalize_automation_claim(AutomationClaimFinalization {
                    project_id: claimed.project_id,
                    project_name: "demo",
                    run_id: 1,
                    claimed_item_id: Some(item_id),
                    agent_id: "agent-test",
                    outcome: AutomationClaimOutcome::Failed,
                    detail: Some("Run failed"),
                })
                .await
                .is_err(),
        };
        assert_that!(&failed).is_true();
        assert_that!(&app.state.items.get("demo", item_id).await.unwrap())
            .is_equal_to(claimed.clone());
        assert_that!(&app.state.comments.list("demo", item_id).await.unwrap())
            .is_equal_to(comments.clone());
        assert_that!(
            &serde_json::to_value(
                app.state
                    .items
                    .events("demo", Some(item_id), None)
                    .await
                    .unwrap()
            )
            .unwrap()
        )
        .is_equal_to(history.clone());
        assert_that!(&events.try_recv().is_err()).is_true();
    }
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "DROP TRIGGER reject_claim_history".to_owned(),
    ))
    .await
    .unwrap();
    app.state
        .claims
        .finish_item(
            "demo",
            item_id,
            "agent-test",
            "Finished",
            Default::default(),
        )
        .await
        .unwrap();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn run_target_resolution_uses_the_callers_single_connection_and_rolls_back_claim_history() {
    use crate::backend::{
        runs::launch::{
            model::{AgentCapabilitySetV1, AgentLaunchResolutionV1, AgentLaunchTargetV1},
            repository::{self as launches, AgentRunLaunchRepository},
        },
        storage::TransactionManager,
    };
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, item_id, _) = crate::backend::comments::tests::application().await;
    let before = app.state.items.get("demo", item_id).await.unwrap();
    let project_id = before.project_id;
    let mut events = app.state.events.subscribe();
    let transactions = TransactionManager::new(&app.state.store);
    let target = AgentLaunchTargetV1::specific(item_id, before.version).unwrap();
    for reject in [true, false] {
        let transaction = transactions.begin().await.unwrap();
        transaction.connection().execute(Statement::from_string(DbBackend::Sqlite,format!("INSERT INTO agent_runs (id,project_id,run_kind,purpose,tool_name,mutability,status,command,working_dir) VALUES (501,{project_id},'task','ordinary','codex','mutating','running','','')"))).await.unwrap();
        launches::insert_contract_in_tx(
            transaction.connection(),
            project_id,
            501,
            dispatch_types::AgentRunPurposeV1::Ordinary,
            &target,
            &AgentCapabilitySetV1::ordinary(),
            &utc_now(),
        )
        .await
        .unwrap();
        if reject {
            transaction.connection().execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER reject_target_resolution BEFORE UPDATE ON agent_run_launch_contracts BEGIN SELECT RAISE(ABORT,'injected resolution failure'); END".to_owned())).await.unwrap();
        }
        let result = app
            .state
            .claims
            .resolve_agent_run_target_in(
                &transaction,
                project_id,
                501,
                &agent_ids::dispatch_run_agent_id(501),
                &target,
                None,
            )
            .await;
        assert_that!(&events.try_recv().is_err()).is_true();
        if reject {
            assert_that!(&result.is_err()).is_true();
            transaction.rollback().await.unwrap();
            assert_that!(&app.state.items.get("demo", item_id).await.unwrap())
                .is_equal_to(before.clone());
            assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()).is_empty();
            assert_that!(
                &AgentRunLaunchRepository
                    .load_in(
                        &crate::backend::storage::TransactionManager::new(&app.state.store)
                            .begin()
                            .await
                            .unwrap(),
                        project_id,
                        501
                    )
                    .await
                    .unwrap()
            )
            .is_none();
        } else {
            let claimed = result.unwrap().unwrap();
            let contract = AgentRunLaunchRepository
                .load_in(&transaction, project_id, 501)
                .await
                .unwrap()
                .unwrap();
            assert_that!(&contract.resolution)
                .is_equal_to(AgentLaunchResolutionV1::claimed(item_id, claimed.version));
            transaction.commit().await.unwrap();
            assert_that!(
                &app.state
                    .items
                    .get("demo", item_id)
                    .await
                    .unwrap()
                    .claimed_by
                    .as_deref()
            )
            .is_equal_to(Some(agent_ids::dispatch_run_agent_id(501).as_str()));
        }
    }
}

#[tokio::test]
async fn second_stale_claim_history_failure_rolls_back_the_recovery_batch() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, first_id, second_id) = crate::backend::comments::tests::application().await;
    for (id, agent) in [(first_id, "agent-first"), (second_id, "agent-second")] {
        app.state
            .claims
            .claim_specific_item("demo", id, agent)
            .await
            .unwrap()
            .unwrap();
    }
    let db = app.state.store.db();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "UPDATE work_items SET claimed_at = '2000-01-01T00:00:00Z' WHERE claimed_by IS NOT NULL"
            .to_owned(),
    ))
    .await
    .unwrap();
    let first = app.state.items.get("demo", first_id).await.unwrap();
    let second = app.state.items.get("demo", second_id).await.unwrap();
    let mut events = app.state.events.subscribe();
    db.execute(Statement::from_string(DbBackend::Sqlite, format!(
        "CREATE TRIGGER reject_second_recovery BEFORE INSERT ON work_item_events WHEN NEW.work_item_id = {second_id} BEGIN SELECT RAISE(ABORT,'injected second recovery failure'); END"))).await.unwrap();
    assert_that!(
        &app.state
            .claims
            .recover_stale_claims("demo", 10)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.items.get("demo", first_id).await.unwrap()).is_equal_to(first);
    assert_that!(&app.state.items.get("demo", second_id).await.unwrap()).is_equal_to(second);
    assert_that!(
        &app.state
            .comments
            .list("demo", first_id)
            .await
            .unwrap()
            .len()
    )
    .is_equal_to(1);
    assert_that!(
        &app.state
            .comments
            .list("demo", second_id)
            .await
            .unwrap()
            .len()
    )
    .is_equal_to(1);
    assert_that!(&events.try_recv().is_err()).is_true();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "DROP TRIGGER reject_second_recovery".to_owned(),
    ))
    .await
    .unwrap();
    let recovered = app
        .state
        .claims
        .recover_stale_claims("demo", 10)
        .await
        .unwrap();
    assert_that!(&recovered.len()).is_equal_to(2);
    assert_that!(
        &app.state
            .items
            .get("demo", first_id)
            .await
            .unwrap()
            .claimed_by
    )
    .is_none();
    assert_that!(
        &app.state
            .items
            .get("demo", second_id)
            .await
            .unwrap()
            .claimed_by
    )
    .is_none();
    events.try_recv().unwrap();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}
