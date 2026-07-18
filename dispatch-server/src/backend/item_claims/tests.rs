use assertr::prelude::*;
use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use tempfile::TempDir;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use super::*;
use crate::backend::{
    agent_ids,
    comments::list_comments,
    entities::{agent_run, work_item::WorkItemActiveModel},
    item_label_service::{add_label, delete_label, update_label},
    items::{CreateWorkItem, create_item, get_item, list_events, list_items, move_item},
    projects::{self, CreateProject, create_project},
    storage::{Store, utc_now},
    work_item_labels, work_items,
};
use crate::shared::view_models::{
    AUTOMATION_BLOCKED_LABEL_KEY, AuthorType, CLAIMED_FROM_STATE_LABEL_KEY, CLAIMED_STATE_LABEL,
    FEEDBACK_REQUESTED_LABEL_KEY, FINISHED_STATE_LABEL, STATE_LABEL_KEY, WorkItemEventType,
};

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    create_project(
        &store,
        CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        },
    )
    .await
    .unwrap();
    create_project(
        &store,
        CreateProject {
            name: "other".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        },
    )
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
    let project_id = projects::project_id(store, project_name).await.unwrap();
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
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Claim me".to_owned(),
            description: "Available work".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();
    let comments = list_comments(&store, "demo", item.id).await.unwrap();
    let events = list_events(&store, "demo", Some(item.id), None)
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
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Progress target".to_owned(),
            description: "Progress should update visible item metadata".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let claimed = claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();

    let comment = progress_item(&store, "demo", item.id, "agent-a", "Working")
        .await
        .unwrap();
    let updated = get_item(&store, "demo", item.id).await.unwrap();
    let events = list_events(&store, "demo", Some(item.id), None)
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
    let (_temp, store) = test_store().await;
    create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Plain item".to_owned(),
            description: "Should not match the selector".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let matching = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Urgent bug".to_owned(),
            description: "Should match the selector".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        matching.id,
        "severity".to_owned(),
        Some("high".to_owned()),
        None,
    )
    .await
    .unwrap();
    add_label(&store, "demo", matching.id, "bug".to_owned(), None, None)
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
        &(has_claimable_item_matching_condition(&store, "demo", &selector)
            .await
            .unwrap())
    )
    .is_true();

    let claimed = claim_item_matching_condition(&store, "demo", "agent-a", &selector)
        .await
        .unwrap()
        .unwrap();

    assert_that!(&(claimed.id)).is_equal_to(matching.id);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some("in_progress"));
}

#[tokio::test]
async fn blocked_items_are_skipped_by_selector_claims() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Blocked bug".to_owned(),
            description: "Should wait for human triage".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
        None,
        None,
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        "severity".to_owned(),
        Some("high".to_owned()),
        None,
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
        &(!has_claimable_item_matching_condition(&store, "demo", &selector)
            .await
            .unwrap())
    )
    .is_true();
    let claimed = claim_item_matching_condition(&store, "demo", "agent-a", &selector)
        .await
        .unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn claimed_from_state_label_is_private_workflow_bookkeeping() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Private claim source".to_owned(),
            description: "Release must trust only workflow-owned claim source labels".to_owned(),
            state: "review".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "review")
        .await
        .unwrap()
        .unwrap();
    let claimed_from_label_id = claimed
        .labels
        .iter()
        .find(|label| label.key == CLAIMED_FROM_STATE_LABEL_KEY)
        .unwrap()
        .id;
    let priority = add_label(
        &store,
        "demo",
        item.id,
        "priority".to_owned(),
        Some("high".to_owned()),
        Some(claimed.version),
    )
    .await
    .unwrap();
    let priority_label_id = priority
        .labels
        .iter()
        .find(|label| label.key == "priority")
        .unwrap()
        .id;

    let add_claim_source = add_label(
        &store,
        "demo",
        item.id,
        CLAIMED_FROM_STATE_LABEL_KEY.to_owned(),
        Some("open".to_owned()),
        Some(priority.version),
    )
    .await
    .unwrap_err();
    assert_that!(
        &(add_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let update_claim_source = update_label(
        &store,
        "demo",
        item.id,
        claimed_from_label_id,
        None,
        Some(Some("open".to_owned())),
        Some(priority.version),
    )
    .await
    .unwrap_err();
    assert_that!(
        &(update_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let rename_to_claim_source = update_label(
        &store,
        "demo",
        item.id,
        priority_label_id,
        Some(CLAIMED_FROM_STATE_LABEL_KEY.to_owned()),
        Some(Some("open".to_owned())),
        Some(priority.version),
    )
    .await
    .unwrap_err();
    assert_that!(
        &(rename_to_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let delete_claim_source = delete_label(
        &store,
        "demo",
        item.id,
        claimed_from_label_id,
        Some(priority.version),
    )
    .await
    .unwrap_err();
    assert_that!(
        &(delete_claim_source
            .to_string()
            .contains("internal workflow bookkeeping"))
    )
    .is_true();

    let released = release_item(
        &store,
        "demo",
        item.id,
        "agent-a",
        Some("done for now".to_owned()),
        ReleaseAutomationDisposition::Blocked,
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("review"));
}

#[tokio::test]
async fn specific_selector_claims_skip_workflow_blockers() {
    let (_temp, store) = test_store().await;
    let selector = open_state_selector();

    for key in [AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY] {
        let item = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: format!("Blocked by {key}"),
                description: "Specific automation claims should still honor blockers".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();
        add_label(&store, "demo", item.id, key.to_owned(), None, None)
            .await
            .unwrap();

        assert_that!(
            &(!has_claimable_specific_item_matching_condition(&store, "demo", item.id, &selector)
                .await
                .unwrap())
        )
        .is_true();
        let claimed =
            claim_specific_item_matching_condition(&store, "demo", item.id, "agent-a", &selector)
                .await
                .unwrap();
        let reloaded = get_item(&store, "demo", item.id).await.unwrap();

        assert_that!(&(claimed.is_none())).is_true();
        assert_that!(&(reloaded.claimed_by)).is_equal_to(None);
        assert_that!(&(reloaded.state.as_deref())).is_equal_to(Some("open"));
    }
}

#[tokio::test]
async fn claiming_is_atomic_for_racing_agents() {
    let (_temp, store) = test_store().await;
    create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Race item".to_owned(),
            description: "Only one agent can own this".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let (first, second) = tokio::join!(
        claim_item(&store, "demo", "agent-a", "open"),
        claim_item(&store, "demo", "agent-b", "open")
    );
    let claims = [first.unwrap(), second.unwrap()];
    let in_progress = list_items(&store, "demo", Some("in_progress".to_owned()))
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
    create_item(
        &store,
        "other",
        CreateWorkItem {
            title: "Other item".to_owned(),
            description: "Should not be claimed from demo".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "open").await.unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn idea_item_is_skipped_until_moved_open() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Draft item".to_owned(),
            description: "Hold this back from automation".to_owned(),
            state: "idea".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let skipped = claim_item(&store, "demo", "agent-a", "open").await.unwrap();
    assert_that!(&(skipped.is_none())).is_true();

    let opened = move_item(
        &store,
        "demo",
        item.id,
        "open".to_owned(),
        Some(item.version),
    )
    .await
    .unwrap();
    let claimed = claim_item(&store, "demo", "agent-a", "open")
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
        create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: title.to_owned(),
                description: "This item should not match an open-state claim".to_owned(),
                state: "idea".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();
    }
    let open = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Open after drafts".to_owned(),
            description: "The scanner should continue until it finds this item".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();

    assert_that!(&(claimed.id)).is_equal_to(open.id);
    assert_that!(&(claimed.claimed_by.as_deref())).is_equal_to(Some("agent-a"));
}

#[tokio::test]
async fn claimed_items_include_verified_automation_source() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Refine me".to_owned(),
            description: "A trigger should be visible while claimed".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
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

    claim_specific_item(&store, "demo", item.id, &agent_id)
        .await
        .unwrap()
        .unwrap();
    let item = get_item(&store, "demo", item.id).await.unwrap();
    let listed = list_items(&store, "demo", None)
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
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Claim me".to_owned(),
            description: "Source should not be guessed from a mismatched run".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let other = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Other".to_owned(),
            description: "The run is structurally linked here instead".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
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

    claim_specific_item(&store, "demo", item.id, &agent_id)
        .await
        .unwrap()
        .unwrap();
    let item = get_item(&store, "demo", item.id).await.unwrap();

    assert_that!(&(item.claimed_by.as_deref())).is_equal_to(Some(agent_id.as_str()));
    assert_that!(&(item.claim_source.is_none())).is_true();
}

#[tokio::test]
async fn release_restores_claim_source_state_and_blocks_automation() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Custom lane item".to_owned(),
            description: "Release should return to this lane".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "ready")
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

    let released = release_item(
        &store,
        "demo",
        item.id,
        "agent-a",
        Some("Cannot operate on this item.".to_owned()),
        ReleaseAutomationDisposition::Blocked,
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

    let claimed_again = claim_item(&store, "demo", "agent-b", "ready")
        .await
        .unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn claimable_release_clears_existing_automation_blocker() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Manual retry".to_owned(),
            description: "A direct retry should be able to reopen automation.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
        None,
        None,
    )
    .await
    .unwrap();

    let claimed = claim_specific_item(&store, "demo", item.id, "agent-a")
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

    let released = release_item(
        &store,
        "demo",
        item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
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

    let claimed_again = claim_item(&store, "demo", "agent-b", "open")
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(claimed_again.id)).is_equal_to(item.id);
    assert_that!(&(claimed_again.claimed_by.as_deref())).is_equal_to(Some("agent-b"));
}

#[tokio::test]
async fn failed_automation_claim_finalization_blocks_retry() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Failed setup".to_owned(),
            description: "Failed automation should not re-enter claim selection immediately."
                .to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-17";
    claim_item(&store, "demo", agent_id, "open")
        .await
        .unwrap()
        .unwrap();

    finalize_automation_claim(
        &store,
        AutomationClaimFinalization {
            project_name: "demo",
            run_id: 17,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("Workspace setup failed\nbecause git was unavailable."),
        },
    )
    .await
    .unwrap();

    let released = get_item(&store, "demo", item.id).await.unwrap();
    let comments = list_comments(&store, "demo", item.id).await.unwrap();

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

    let claimed_again = claim_item(&store, "demo", "agent-b", "open").await.unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn successful_and_cancelled_automation_claim_finalization_remains_claimable() {
    let (_temp, store) = test_store().await;

    for (index, outcome) in [
        (1, AutomationClaimOutcome::CompletedUnfinished),
        (2, AutomationClaimOutcome::Cancelled),
    ] {
        let source_state = format!("ready-{index}");
        let item = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: format!("Claimable outcome {index}"),
                description: "Non-failed automation outcomes should release without blockers."
                    .to_owned(),
                state: source_state.clone(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();
        add_label(
            &store,
            "demo",
            item.id,
            AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
            None,
            None,
        )
        .await
        .unwrap();
        let agent_id = format!("dispatch-run-{}", 20 + index);
        claim_specific_item(&store, "demo", item.id, &agent_id)
            .await
            .unwrap()
            .unwrap();

        finalize_automation_claim(
            &store,
            AutomationClaimFinalization {
                project_name: "demo",
                run_id: 20 + index,
                claimed_item_id: Some(item.id),
                agent_id: &agent_id,
                outcome,
                detail: None,
            },
        )
        .await
        .unwrap();

        let released = get_item(&store, "demo", item.id).await.unwrap();
        assert_that!(&(released.state.as_deref())).is_equal_to(Some(source_state.as_str()));
        assert_that!(&(released.claimed_by)).is_equal_to(None);
        assert_that!(
            &(released
                .labels
                .iter()
                .all(|label| label.key != AUTOMATION_BLOCKED_LABEL_KEY))
        )
        .is_true();

        let claimed_again = claim_item(&store, "demo", "agent-b", &source_state)
            .await
            .unwrap()
            .unwrap();
        assert_that!(&(claimed_again.id)).is_equal_to(item.id);
        release_item(
            &store,
            "demo",
            item.id,
            "agent-b",
            None,
            ReleaseAutomationDisposition::Claimable,
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn automation_claim_finalization_uses_finish_metadata_not_state_label() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Moved to done".to_owned(),
            description: "A done state label alone should not suppress claim cleanup.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-31";
    claim_item(&store, "demo", agent_id, "open")
        .await
        .unwrap()
        .unwrap();
    move_item(
        &store,
        "demo",
        item.id,
        FINISHED_STATE_LABEL.to_owned(),
        None,
    )
    .await
    .unwrap();

    finalize_automation_claim(
        &store,
        AutomationClaimFinalization {
            project_name: "demo",
            run_id: 31,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("Run failed after the state label changed."),
        },
    )
    .await
    .unwrap();

    let released = get_item(&store, "demo", item.id).await.unwrap();
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
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Already finished".to_owned(),
            description: "Finished timestamp is the terminal item marker.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let agent_id = "dispatch-run-41";
    claim_item(&store, "demo", agent_id, "open")
        .await
        .unwrap()
        .unwrap();

    let model = work_items::get(store.db().as_ref(), item.project_id, item.id)
        .await
        .unwrap();
    let mut active: WorkItemActiveModel = model.into();
    active.finished_at = Set(Some(utc_now()));
    active.update(store.db().as_ref()).await.unwrap();

    finalize_automation_claim(
        &store,
        AutomationClaimFinalization {
            project_name: "demo",
            run_id: 41,
            claimed_item_id: Some(item.id),
            agent_id,
            outcome: AutomationClaimOutcome::Failed,
            detail: Some("This should be ignored."),
        },
    )
    .await
    .unwrap();

    let current = get_item(&store, "demo", item.id).await.unwrap();
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
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Needs input".to_owned(),
            description: "Agent should ask for a user decision".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "ready")
        .await
        .unwrap()
        .unwrap();

    let updated = request_feedback(
        &store,
        "demo",
        item.id,
        "agent-a",
        "Which provider should this integration target?",
    )
    .await
    .unwrap();
    let comments = list_comments(&store, "demo", item.id).await.unwrap();
    let events = list_events(&store, "demo", Some(item.id), None)
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

    let claimed_again = claim_item(&store, "demo", "agent-b", "ready")
        .await
        .unwrap();
    assert_that!(&(claimed_again.is_none())).is_true();
}

#[tokio::test]
async fn feedback_requested_label_blocks_state_claims() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Awaiting answer".to_owned(),
            description: "Feedback label alone should block automation pickup".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
        None,
        None,
    )
    .await
    .unwrap();

    let claimed = claim_item(&store, "demo", "agent-a", "open").await.unwrap();

    assert_that!(&(claimed.is_none())).is_true();
}

#[tokio::test]
async fn specific_claim_release_restores_current_state() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Manual retry".to_owned(),
            description: "Explicit item claims are not tied to open".to_owned(),
            state: "triage".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let claimed = claim_specific_item(&store, "demo", item.id, "agent-a")
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(claimed.state.as_deref())).is_equal_to(Some(CLAIMED_STATE_LABEL));

    let released = release_item(
        &store,
        "demo",
        item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
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

    let claimed_again = claim_item(&store, "demo", "agent-b", "triage")
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(claimed_again.id)).is_equal_to(item.id);
    assert_that!(&(claimed_again.claimed_by.as_deref())).is_equal_to(Some("agent-b"));
}

#[tokio::test]
async fn new_claims_overwrite_stale_claim_source_with_current_state() {
    let (_temp, store) = test_store().await;
    let state_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "State retry".to_owned(),
            description: "State claims use the current state as release source".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", state_item.id, "ready").await;

    let claimed = claim_item(&store, "demo", "agent-state", "open")
        .await
        .unwrap()
        .unwrap();

    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("open")
        }))
    )
    .is_true();

    let released = release_item(
        &store,
        "demo",
        state_item.id,
        "agent-state",
        None,
        ReleaseAutomationDisposition::Claimable,
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));

    let selector_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Selector retry".to_owned(),
            description: "Claim source should come from the current state label".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", selector_item.id, "open").await;

    let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String("ready".to_owned()),
    })]);
    let claimed = claim_item_matching_condition(&store, "demo", "agent-a", &selector)
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

    let released = release_item(
        &store,
        "demo",
        selector_item.id,
        "agent-a",
        None,
        ReleaseAutomationDisposition::Claimable,
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("ready"));

    let specific_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Specific retry".to_owned(),
            description: "Specific claims use the same source-state rule".to_owned(),
            state: "triage".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    seed_claim_source_label(&store, "demo", specific_item.id, "open").await;

    let claimed = claim_specific_item(&store, "demo", specific_item.id, "agent-b")
        .await
        .unwrap()
        .unwrap();

    assert_that!(
        &(claimed.labels.iter().any(|label| {
            label.key == CLAIMED_FROM_STATE_LABEL_KEY && label.value.as_deref() == Some("triage")
        }))
    )
    .is_true();

    let released = release_item(
        &store,
        "demo",
        specific_item.id,
        "agent-b",
        None,
        ReleaseAutomationDisposition::Claimable,
    )
    .await
    .unwrap();

    assert_that!(&(released.state.as_deref())).is_equal_to(Some("triage"));
}

#[tokio::test]
async fn release_requires_current_claimant() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Owned item".to_owned(),
            description: "Only the claimant can release it".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();

    let err = release_item(
        &store,
        "demo",
        item.id,
        "agent-b",
        None,
        ReleaseAutomationDisposition::Blocked,
    )
    .await
    .unwrap_err();

    assert_that!(&(err.to_string().contains("claimed by agent-a"))).is_true();
}

#[tokio::test]
async fn finish_clears_claim_and_blocking_workflow_labels() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Finish blocked item".to_owned(),
            description: "Completion should clear workflow bookkeeping labels".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        AUTOMATION_BLOCKED_LABEL_KEY.to_owned(),
        None,
        None,
    )
    .await
    .unwrap();
    claim_specific_item(&store, "demo", item.id, "agent-a")
        .await
        .unwrap()
        .unwrap();
    add_label(
        &store,
        "demo",
        item.id,
        FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
        None,
        None,
    )
    .await
    .unwrap();

    let finished = finish_item(&store, "demo", item.id, "agent-a", "Finished cleanly")
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
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Finish item".to_owned(),
            description: "Complete with report".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();

    let finished = finish_item(&store, "demo", item.id, "agent-a", "Finished cleanly")
        .await
        .unwrap();
    let comments = list_comments(&store, "demo", item.id).await.unwrap();
    let events = list_events(&store, "demo", Some(item.id), None)
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
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Finished item".to_owned(),
            description: "State changes alone should not reopen finished work".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();
    let finished = finish_item(&store, "demo", item.id, "agent-a", "Finished")
        .await
        .unwrap();
    let moved = move_item(
        &store,
        "demo",
        item.id,
        "open".to_owned(),
        Some(finished.version),
    )
    .await
    .unwrap();
    let selector = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String("open".to_owned()),
    })]);

    let state_claim = claim_item(&store, "demo", "agent-b", "open").await.unwrap();
    let selector_has_match = has_claimable_item_matching_condition(&store, "demo", &selector)
        .await
        .unwrap();
    let selector_claim = claim_item_matching_condition(&store, "demo", "agent-c", &selector)
        .await
        .unwrap();
    let reloaded = get_item(&store, "demo", item.id).await.unwrap();

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
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Finished item".to_owned(),
            description: "Should stay closed after completion".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();
    finish_item(&store, "demo", item.id, "agent-a", "Finished")
        .await
        .unwrap();

    let claimed = claim_specific_item(&store, "demo", item.id, "agent-b")
        .await
        .unwrap();
    let reloaded = get_item(&store, "demo", item.id).await.unwrap();

    assert_that!(&(claimed.is_none())).is_true();
    assert_that!(&(reloaded.state.as_deref())).is_equal_to(Some(FINISHED_STATE_LABEL));
    assert_that!(&(reloaded.claimed_by)).is_equal_to(None);
    assert_that!(&(reloaded.finished_at.is_some())).is_true();
}

#[tokio::test]
async fn stale_claim_recovery_releases_old_claim() {
    let (_temp, store) = test_store().await;
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Stale item".to_owned(),
            description: "Claim should be recovered".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    claim_item(&store, "demo", "agent-a", "open")
        .await
        .unwrap()
        .unwrap();
    let project_id = projects::project_id(&store, "demo").await.unwrap();
    let mut model: WorkItemActiveModel = work_items::get(store.db().as_ref(), project_id, item.id)
        .await
        .unwrap()
        .into();
    model.claimed_at = Set(Some(
        (OffsetDateTime::now_utc() - Duration::minutes(30))
            .format(&Rfc3339)
            .unwrap(),
    ));
    model.update(store.db().as_ref()).await.unwrap();

    let recovered = recover_stale_claims(&store, "demo", 10).await.unwrap();
    let item = get_item(&store, "demo", item.id).await.unwrap();

    assert_that!(&(recovered.len())).is_equal_to(1);
    assert_that!(&(recovered[0].agent_id)).is_equal_to("agent-a");
    assert_that!(&(item.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(&(item.claimed_by)).is_equal_to(None);
}
