use super::Condition;
use assertr::prelude::*;
use std::path::PathBuf;

use crudkit_core::condition::{ConditionClause, ConditionClauseValue, ConditionElement, Operator};
use tempfile::TempDir;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use super::*;
use crate::backend::{
    agent_tools::set_tool_path,
    automation, automation_revisions, automation_routing, item_claims,
    item_label_service::add_label,
    items::{self, CreateWorkItem, create_item, get_item},
    label_conditions, personalities,
    projects::{CreateProject, create_project},
};
use crate::shared::view_models::{
    AUTOMATION_BLOCKED_LABEL_KEY, AutomationEvaluationOutcome, AutomationExecutionPolicy,
    CreateWorkItemLabelRequest, FEEDBACK_REQUESTED_LABEL_KEY, ProduceDeduplication,
    ProducedWorkSpec, RoutingExplainRequest, STATE_LABEL_KEY, WorkItemOriginKind, WorkItemView,
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
    set_tool_path(&store, AgentToolName::Codex, PathBuf::from("/bin/echo"))
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

fn routed_open_selector(route: &str) -> Condition {
    Condition::All(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String("open".to_owned()),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: "route".to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String(route.to_owned()),
        }),
    ])
}

fn item_matches_selector(item: &WorkItemView, selector: &Condition) -> bool {
    label_conditions::ValidatedLabelCondition::new(selector)
        .unwrap()
        .matches(&item.labels)
}

async fn producer_view(
    store: &Store,
    name: &str,
    deduplication: ProduceDeduplication,
) -> AutomationTriggerView {
    let mut trigger = create_trigger(
        store,
        "demo",
        CreateAutomationTrigger {
            name: name.to_owned(),
            enabled: true,
            activation: AutomationActivation::Manual,
            effect: AutomationEffect::ProduceWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Produced item description.".to_owned(),
            work_item_selector: None,
            priority: 0,
        },
    )
    .await
    .unwrap();
    trigger.produced_work = Some(ProducedWorkSpec {
        title: Some(format!("{name} item")),
        state: "open".to_owned(),
        initial_labels: vec![CreateWorkItemLabelRequest {
            key: "source".to_owned(),
            value: Some("automation".to_owned()),
        }],
        agent_model_override: None,
        agent_reasoning_effort_override: None,
        deduplication,
    });
    trigger
}

#[test]
fn schedules_accept_every_notation() {
    assert_that!(&(parse_schedule("@every 15m").is_ok())).is_true();
    assert_that!(&(parse_schedule("@hourly").is_ok())).is_true();
    assert_that!(&(parse_schedule("0s").is_err())).is_true();
}

#[tokio::test]
async fn produced_work_applies_fields_and_always_creates() {
    let (_temp, store) = test_store().await;
    let trigger = producer_view(&store, "campaign", ProduceDeduplication::Always).await;

    let first = create_work_item_from_trigger(&store, "demo", &trigger)
        .await
        .unwrap();
    let second = create_work_item_from_trigger(&store, "demo", &trigger)
        .await
        .unwrap();

    assert_that!(&(first.id)).is_not_equal_to(second.id);
    assert_that!(&(first.title)).is_equal_to("campaign item");
    assert_that!(&(first.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(
        &(first.labels.iter().any(|label| {
            label.key == "source" && label.value.as_deref() == Some("automation")
        }))
    )
    .is_true();
    let origin = first.origin.unwrap();
    assert_that!(&(origin.kind)).is_equal_to(WorkItemOriginKind::ProducingAutomation);
    assert_that!(&(origin.trigger_id)).is_equal_to(Some(trigger.id));
    assert_that!(&(origin.trigger_revision_id)).is_equal_to(trigger.current_revision_id);
    assert_that!(&(origin.producing_evaluation_id.is_some())).is_true();
}

#[tokio::test]
async fn produced_work_deduplicates_by_trigger_until_completion() {
    let (_temp, store) = test_store().await;
    let trigger = producer_view(
        &store,
        "single-flight",
        ProduceDeduplication::WhileUnfinishedForTrigger,
    )
    .await;

    let first = create_work_item_from_trigger(&store, "demo", &trigger)
        .await
        .unwrap();
    let duplicate = create_work_item_from_trigger(&store, "demo", &trigger)
        .await
        .unwrap();
    assert_that!(&(duplicate.id)).is_equal_to(first.id);

    item_claims::claim_specific_item(&store, "demo", first.id, "agent-test")
        .await
        .unwrap()
        .unwrap();
    item_claims::finish_item(&store, "demo", first.id, "agent-test", "complete")
        .await
        .unwrap();
    let replacement = create_work_item_from_trigger(&store, "demo", &trigger)
        .await
        .unwrap();
    assert_that!(&(replacement.id)).is_not_equal_to(first.id);

    let evaluations = automation_revisions::list_evaluations(&store, "demo", Some(trigger.id), 20)
        .await
        .unwrap();
    assert_that!(&(evaluations.len())).is_equal_to(3);
    assert_that!(
        &(evaluations.iter().any(|evaluation| {
            evaluation.outcome == AutomationEvaluationOutcome::SkippedDuplicate
                && evaluation.work_item_id == Some(first.id)
        }))
    )
    .is_true();
}

#[tokio::test]
async fn produced_work_key_deduplication_spans_triggers_and_is_concurrency_safe() {
    let (_temp, store) = test_store().await;
    let key = "campaign.plan".to_owned();
    let first_trigger = producer_view(
        &store,
        "first-producer",
        ProduceDeduplication::WhileUnfinishedForKey { key: key.clone() },
    )
    .await;
    let second_trigger = producer_view(
        &store,
        "second-producer",
        ProduceDeduplication::WhileUnfinishedForKey { key },
    )
    .await;

    let (first, concurrent) = tokio::join!(
        create_work_item_from_trigger(&store, "demo", &first_trigger),
        create_work_item_from_trigger(&store, "demo", &first_trigger),
    );
    let first = first.unwrap();
    assert_that!(&(concurrent.unwrap().id)).is_equal_to(first.id);

    let cross_trigger = create_work_item_from_trigger(&store, "demo", &second_trigger)
        .await
        .unwrap();
    assert_that!(&(cross_trigger.id)).is_equal_to(first.id);
    assert_that!(&(items::list_items(&store, "demo", None).await.unwrap().len())).is_equal_to(1);
}

#[tokio::test]
async fn new_project_gets_default_work_item_automation() {
    let (_temp, store) = test_store().await;
    let automations = list_triggers(&store, "demo").await.unwrap();
    assert_that!(&(automations.len())).is_equal_to(3);
    let automation = automation_by_name(&automations, DEFAULT_WORK_ITEM_AUTOMATION_NAME);

    assert_that!(&(automation.activation)).is_equal_to(AutomationActivation::WorkItem);
    assert_that!(&(automation.effect)).is_equal_to(AutomationEffect::ConsumeWork);
    assert_that!(&(automation.mutability)).is_equal_to(AutomationRunMutability::Mutating);
    assert_that!(&(automation.schedule)).is_equal_to(DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE);
    assert_that!(&(automation.work_item_selector)).is_equal_to(Some(default_work_item_selector()));
    assert_that!(&(automation.priority)).is_equal_to(0);
    assert_that!(&(automation.evaluation_count)).is_equal_to(0);
    assert_that!(&(automation.pending_evaluation_count)).is_equal_to(0);

    let refiner = automation_by_name(&automations, DEFAULT_REFINEMENT_AUTOMATION_NAME);
    assert_that!(&(refiner.activation)).is_equal_to(AutomationActivation::WorkItem);
    assert_that!(&(refiner.effect)).is_equal_to(AutomationEffect::ConsumeWork);
    assert_that!(&(refiner.mutability)).is_equal_to(AutomationRunMutability::ReadOnly);
    assert_that!(&(refiner.schedule)).is_equal_to(DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE);
    assert_that!(&(refiner.work_item_selector))
        .is_equal_to(Some(default_refinement_work_item_selector()));
    assert_that!(&(refiner.priority)).is_equal_to(REFINEMENT_AUTOMATION_PRIORITY);
    assert_that!(&(refiner.prompt.contains("Do not implement the work"))).is_true();
    assert_that!(
        &(refiner
            .prompt
            .contains("Remove the `needs-refinement` label"))
    )
    .is_true();
    assert_that!(
        &(refiner
            .prompt
            .contains("Do not call `dispatch item finish`"))
    )
    .is_true();

    let verifier = automation_by_name(&automations, DEFAULT_VERIFICATION_AUTOMATION_NAME);
    assert_that!(&(verifier.activation)).is_equal_to(AutomationActivation::WorkItem);
    assert_that!(&(verifier.effect)).is_equal_to(AutomationEffect::ConsumeWork);
    assert_that!(&(verifier.mutability)).is_equal_to(AutomationRunMutability::ReadOnly);
    assert_that!(&(verifier.schedule)).is_equal_to(DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE);
    assert_that!(&(verifier.work_item_selector))
        .is_equal_to(Some(default_verification_work_item_selector()));
    assert_that!(&(verifier.priority)).is_equal_to(VERIFICATION_AUTOMATION_PRIORITY);
    assert_that!(&(verifier.prompt.contains("Do not implement the work"))).is_true();
    assert_that!(
        &(verifier
            .prompt
            .contains("Remove the `needs-verification` label"))
    )
    .is_true();
    assert_that!(
        &(verifier
            .prompt
            .contains("do not invent or hardcode a state name"))
    )
    .is_true();
}

fn automation_by_name<'a>(
    automations: &'a [AutomationTriggerView],
    name: &str,
) -> &'a AutomationTriggerView {
    automations
        .iter()
        .find(|automation| automation.name == name)
        .unwrap()
}

#[tokio::test]
async fn trigger_create_and_update_round_trip_mutability() {
    let (_temp, store) = test_store().await;
    let default_personality = personalities::list_personalities(&store, "demo")
        .await
        .unwrap()[0]
        .clone();
    let trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "read-only-review".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Review metadata.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: 5,
        },
    )
    .await
    .unwrap();

    assert_that!(&(trigger.mutability)).is_equal_to(AutomationRunMutability::ReadOnly);
    assert_that!(&(trigger.personality_id)).is_equal_to(Some(default_personality.id));
    let updated = update_trigger(
        &store,
        "demo",
        trigger.id,
        UpdateAutomationTrigger {
            name: "mutating-review".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            mutability: AutomationRunMutability::Mutating,
            personality_id: None,
            prompt: "Review and edit.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: Some(6),
        },
    )
    .await
    .unwrap();

    assert_that!(&(updated.mutability)).is_equal_to(AutomationRunMutability::Mutating);
    assert_that!(&(updated.personality_id)).is_equal_to(Some(default_personality.id));
    assert_that!(&(updated.priority)).is_equal_to(6);
}

#[tokio::test]
async fn restoring_a_trigger_revision_appends_new_immutable_history() {
    let (_temp, store) = test_store().await;
    let trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "revision-target".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Original prompt.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: 1,
        },
    )
    .await
    .unwrap();
    let updated = update_trigger(
        &store,
        "demo",
        trigger.id,
        UpdateAutomationTrigger {
            name: "revision-target-updated".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 30s".to_owned(),
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: trigger.personality_id,
            prompt: "Updated prompt.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: Some(2),
        },
    )
    .await
    .unwrap();
    assert_that!(&(updated.current_revision_id)).is_not_equal_to(trigger.current_revision_id);

    let revisions =
        automation_revisions::list_trigger_revisions(&store, trigger.project_id, trigger.id)
            .await
            .unwrap();
    assert_that!(&(revisions.len())).is_equal_to(2);
    let original = revisions
        .iter()
        .find(|revision| revision.revision_number == 1)
        .unwrap();
    let restored =
        automation_revisions::restore_trigger_revision(&store, "demo", trigger.id, original.id)
            .await
            .unwrap();
    assert_that!(&(restored.name)).is_equal_to("revision-target");
    assert_that!(&(restored.prompt)).is_equal_to("Original prompt.");

    let revisions =
        automation_revisions::list_trigger_revisions(&store, trigger.project_id, trigger.id)
            .await
            .unwrap();
    assert_that!(&(revisions.len())).is_equal_to(3);
    assert_that!(&(revisions[0].revision_number)).is_equal_to(3);
    assert_that!(&(revisions[0].operation)).is_equal_to(RevisionChangeOperation::Restore);
    assert_that!(&(restored.current_revision_id)).is_equal_to(Some(revisions[0].id));
}

#[tokio::test]
async fn trigger_rejects_cross_project_personality() {
    let (temp, store) = test_store().await;
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
    let other_default = personalities::list_personalities(&store, "other")
        .await
        .unwrap()[0]
        .id;

    let err = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "bad-personality".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: Some(other_default),
            prompt: "Review metadata.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: 5,
        },
    )
    .await
    .unwrap_err();

    assert_that!(&(err.to_string().contains("does not exist in this project"))).is_true();
}

#[tokio::test]
async fn default_selectors_route_labeled_items_to_refinement_automations() {
    let (_temp, store) = test_store().await;
    let refine_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Needs shape".to_owned(),
            description: "Rough story".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let refine_item = add_label(
        &store,
        "demo",
        refine_item.id,
        "needs-refinement".to_owned(),
        None,
        None,
    )
    .await
    .unwrap();
    let verify_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Needs check".to_owned(),
            description: "Verify this before implementation".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();
    let verify_item = add_label(
        &store,
        "demo",
        verify_item.id,
        "needs-verification".to_owned(),
        None,
        None,
    )
    .await
    .unwrap();

    assert_that!(&(!item_matches_selector(&refine_item, &default_work_item_selector()))).is_true();
    assert_that!(&(item_matches_selector(&refine_item, &default_refinement_work_item_selector())))
        .is_true();
    assert_that!(&(!item_matches_selector(&verify_item, &default_work_item_selector()))).is_true();
    assert_that!(
        &(item_matches_selector(&verify_item, &default_verification_work_item_selector()))
    )
    .is_true();
}

#[test]
fn work_item_automation_score_combines_age_evaluation_count_and_priority() {
    let now = OffsetDateTime::now_utc();
    let stale_low_priority = automation_view_for_score(
        1,
        Some((now - Duration::minutes(30)).format(&Rfc3339).unwrap()),
        10,
        0,
    );
    let recent_lower_evaluation_count = automation_view_for_score(
        2,
        Some((now - Duration::minutes(1)).format(&Rfc3339).unwrap()),
        8,
        0,
    );
    let recent_high_priority = automation_view_for_score(
        3,
        Some((now - Duration::minutes(1)).format(&Rfc3339).unwrap()),
        10,
        10,
    );

    assert_that!(
        &(work_item_automation_score(&stale_low_priority, 10, now)
            > work_item_automation_score(&recent_lower_evaluation_count, 10, now))
    )
    .is_true();
    assert_that!(
        &(work_item_automation_score(&recent_lower_evaluation_count, 10, now)
            > work_item_automation_score(&recent_high_priority, 10, now)
                - (10 * PRIORITY_SCORE_SECONDS))
    )
    .is_true();
    assert_that!(
        &(work_item_automation_score(&recent_high_priority, 10, now)
            > work_item_automation_score(&recent_lower_evaluation_count, 10, now))
    )
    .is_true();
}

#[tokio::test]
async fn work_item_automation_skips_stale_candidate_and_tries_next() {
    let (_temp, store) = test_store().await;
    let first_trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "first-route".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Inspect first route.".to_owned(),
            work_item_selector: Some(routed_open_selector("first")),
            priority: 10,
        },
    )
    .await
    .unwrap();
    let second_trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "second-route".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Inspect second route.".to_owned(),
            work_item_selector: Some(routed_open_selector("second")),
            priority: 0,
        },
    )
    .await
    .unwrap();
    let first_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "First route".to_owned(),
            description: "This item becomes stale after candidate selection.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: vec![CreateWorkItemLabelRequest {
                key: "route".to_owned(),
                value: Some("first".to_owned()),
            }],
        },
    )
    .await
    .unwrap();
    let second_item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Second route".to_owned(),
            description: "This item should still be available.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: vec![CreateWorkItemLabelRequest {
                key: "route".to_owned(),
                value: Some("second".to_owned()),
            }],
        },
    )
    .await
    .unwrap();
    let first_model = AutomationTrigger::find_by_id(first_trigger.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    let second_model = AutomationTrigger::find_by_id(second_trigger.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    item_claims::claim_specific_item(&store, "demo", first_item.id, "agent-other")
        .await
        .unwrap()
        .unwrap();

    let outcome = run_first_available_work_item_automation_candidate(
        &store,
        "demo",
        vec![
            WorkItemAutomationCandidate {
                trigger: first_model,
                view: first_trigger.clone(),
                item_ids: vec![first_item.id],
            },
            WorkItemAutomationCandidate {
                trigger: second_model,
                view: second_trigger.clone(),
                item_ids: vec![second_item.id],
            },
        ],
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    let first_item = get_item(&store, "demo", first_item.id).await.unwrap();

    assert_that!(&(outcome.trigger_id)).is_equal_to(second_trigger.id);
    assert_that!(&(first_item.claimed_by.as_deref())).is_equal_to(Some("agent-other"));
}

#[tokio::test]
async fn exclusive_routing_uses_strict_priority_and_matches_diagnostics() {
    let (_temp, store) = test_store().await;
    let lower = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "lower-exclusive".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Lower priority route.".to_owned(),
            work_item_selector: Some(routed_open_selector("exclusive")),
            priority: 10,
        },
    )
    .await
    .unwrap();
    let higher = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "higher-exclusive".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItem,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Higher priority route.".to_owned(),
            work_item_selector: Some(routed_open_selector("exclusive")),
            priority: 20,
        },
    )
    .await
    .unwrap();
    for id in [lower.id, higher.id] {
        let model = AutomationTrigger::find_by_id(id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap();
        let mut active: AutomationTriggerActiveModel = model.into();
        active.exclusive = Set(true);
        active.update(store.db().as_ref()).await.unwrap();
    }
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "Exclusive route".to_owned(),
            description: "Only the strict-priority winner should run.".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: vec![CreateWorkItemLabelRequest {
                key: "route".to_owned(),
                value: Some("exclusive".to_owned()),
            }],
        },
    )
    .await
    .unwrap();

    let explanation = automation_routing::explain(
        &store,
        "demo",
        RoutingExplainRequest {
            item_id: Some(item.id),
            rule: None,
        },
    )
    .await
    .unwrap();
    assert_that!(&(explanation.winner_trigger_id)).is_equal_to(Some(higher.id));
    assert_that!(
        &(explanation.rules.iter().any(|rule| {
            rule.trigger_id != Some(lower.id)
                && !rule.exclusive
                && rule.selector_matches
                && rule.suppressed_by_exclusive
        }))
    )
    .is_true();
    assert_that!(
        &(explanation
            .rules
            .iter()
            .filter(|rule| !rule.selector_matches)
            .all(|rule| !rule.suppressed_by_exclusive))
    )
    .is_true();

    let outcome = run_next_work_item_automation_for_project(&store, "demo", None, None, None)
        .await
        .unwrap()
        .unwrap();
    assert_that!(&(outcome.trigger_id)).is_equal_to(explanation.winner_trigger_id.unwrap());
}

fn automation_view_for_score(
    id: i64,
    last_evaluated_at: Option<String>,
    evaluation_count: i64,
    priority: i64,
) -> AutomationTriggerView {
    AutomationTriggerView {
        id,
        project_id: 1,
        name: format!("automation-{id}"),
        enabled: true,
        activation: AutomationActivation::WorkItem,
        effect: AutomationEffect::ConsumeWork,
        schedule: DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE.to_owned(),
        tool_name: AgentToolName::Codex,
        mutability: AutomationRunMutability::Mutating,
        personality_id: Some(1),
        personality_name: Some(personalities::DEFAULT_PERSONALITY_NAME.to_owned()),
        prompt: String::new(),
        work_item_selector: Some(default_work_item_selector()),
        priority,
        exclusive: false,
        produced_work: None,
        execution: AutomationExecutionPolicy::default(),
        postconditions: None,
        current_revision_id: None,
        managed_bundle_key: None,
        managed_object_key: None,
        evaluation_count,
        pending_evaluation_count: 0,
        last_evaluation_queued_at: None,
        last_evaluated_at,
        next_evaluation_at: None,
        last_event_id: None,
        created_at: "2026-06-15T00:00:00Z".to_owned(),
        updated_at: "2026-06-15T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn work_item_created_trigger_targets_new_item() {
    let (_temp, store) = test_store().await;
    create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "refine-new-work".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItemCreated,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Refine this new work item.".to_owned(),
            work_item_selector: Some(default_work_item_selector()),
            priority: 0,
        },
    )
    .await
    .unwrap();
    let item = create_item(
        &store,
        "demo",
        CreateWorkItem {
            title: "New item".to_owned(),
            description: "Trigger should target this item".to_owned(),
            state: "open".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
    )
    .await
    .unwrap();

    let outcomes = run_due_triggers(&store).await.unwrap();
    let item = get_item(&store, "demo", item.id).await.unwrap();
    let triggers = list_triggers(&store, "demo").await.unwrap();
    let trigger = triggers
        .iter()
        .find(|trigger| trigger.name == "refine-new-work")
        .unwrap();

    assert_that!(&(outcomes.len())).is_equal_to(1);

    let run = outcomes[0].run.as_ref().unwrap();
    let trigger_runs = automation::list_runs_for_trigger(&store, "demo", trigger.id, None)
        .await
        .unwrap();

    assert_that!(&(outcomes[0].work_item_id)).is_equal_to(Some(item.id));
    assert_that!(&(outcomes[0].run.is_some())).is_true();
    assert_that!(&(run.trigger_id)).is_equal_to(Some(trigger.id));
    assert_that!(&(run.trigger_name.as_deref())).is_equal_to(Some("refine-new-work"));
    assert_that!(&(run.mutability)).is_equal_to(AutomationRunMutability::ReadOnly);
    assert_that!(&(trigger_runs.len())).is_equal_to(1);
    assert_that!(&(trigger_runs[0].id)).is_equal_to(run.id);
    assert_that!(&(item.claimed_by)).is_equal_to(None);
    assert_that!(&(item.state.as_deref())).is_equal_to(Some("open"));
    assert_that!(
        &(item
            .labels
            .iter()
            .all(|label| label.key != crate::shared::view_models::AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
}

#[tokio::test]
async fn work_item_created_trigger_skips_specific_items_blocked_from_automation() {
    let (_temp, store) = test_store().await;
    let trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "inspect-new-work".to_owned(),
            enabled: true,
            activation: AutomationActivation::WorkItemCreated,
            effect: AutomationEffect::ConsumeWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::ReadOnly,
            personality_id: None,
            prompt: "Inspect this new work item.".to_owned(),
            work_item_selector: Some(open_state_selector()),
            priority: 0,
        },
    )
    .await
    .unwrap();

    for key in [AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY] {
        create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: format!("Blocked by {key}"),
                description: "A matching selector should still respect automation blockers."
                    .to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![CreateWorkItemLabelRequest {
                    key: key.to_owned(),
                    value: None,
                }],
            },
        )
        .await
        .unwrap();
    }

    let outcomes = run_due_triggers(&store).await.unwrap();
    let trigger_runs = automation::list_runs_for_trigger(&store, "demo", trigger.id, None)
        .await
        .unwrap();

    assert_that!(&(outcomes.is_empty())).is_true();
    assert_that!(&(trigger_runs.is_empty())).is_true();
}

#[tokio::test]
async fn queued_work_producing_trigger_creates_item_without_agent_run() {
    let (_temp, store) = test_store().await;
    let trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "deep-review".to_owned(),
            enabled: true,
            activation: AutomationActivation::Manual,
            effect: AutomationEffect::ProduceWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::Mutating,
            personality_id: None,
            prompt: "Perform an expensive deep review.".to_owned(),
            work_item_selector: None,
            priority: 100,
        },
    )
    .await
    .unwrap();
    let trigger_id = trigger.id;

    let queued = schedule_trigger_evaluation(&store, "demo", trigger_id)
        .await
        .unwrap();
    assert_that!(&(queued.pending_evaluation_count)).is_equal_to(1);

    let outcomes = run_due_triggers(&store).await.unwrap();
    assert_that!(&(outcomes.len())).is_equal_to(1);
    assert_that!(&(outcomes[0].trigger_id)).is_equal_to(trigger_id);
    assert_that!(&(outcomes[0].run.is_none())).is_true();

    let work_item = outcomes[0].work_item.as_ref().unwrap();
    assert_that!(&(outcomes[0].work_item_id)).is_equal_to(Some(work_item.id));
    assert_that!(&(work_item.title)).is_equal_to("deep-review");
    assert_that!(&(work_item.description)).is_equal_to("Perform an expensive deep review.");

    let trigger = list_triggers(&store, "demo")
        .await
        .unwrap()
        .into_iter()
        .find(|trigger| trigger.id == trigger_id)
        .unwrap();
    assert_that!(&(trigger.pending_evaluation_count)).is_equal_to(0);
    assert_that!(&(trigger.evaluation_count)).is_equal_to(1);
}

#[tokio::test]
async fn queued_evaluations_wait_until_project_is_active() {
    let (temp, store) = test_store().await;
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
    let trigger = create_trigger(
        &store,
        "other",
        CreateAutomationTrigger {
            name: "other-project-review".to_owned(),
            enabled: true,
            activation: AutomationActivation::Manual,
            effect: AutomationEffect::ProduceWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::Mutating,
            personality_id: None,
            prompt: "Review work in the other project.".to_owned(),
            work_item_selector: None,
            priority: 100,
        },
    )
    .await
    .unwrap();
    schedule_trigger_evaluation(&store, "other", trigger.id)
        .await
        .unwrap();

    let active_project_ids = vec![projects::project_id(&store, "demo").await.unwrap()];
    let outcomes = run_due_triggers_with_sessions_for_projects(
        &store,
        None,
        None,
        Some(&active_project_ids),
        None,
    )
    .await
    .unwrap();
    let trigger = list_triggers(&store, "other")
        .await
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == trigger.id)
        .unwrap();

    assert_that!(&(outcomes.is_empty())).is_true();
    assert_that!(&(trigger.pending_evaluation_count)).is_equal_to(1);
    assert_that!(&(trigger.evaluation_count)).is_equal_to(0);
}

#[tokio::test]
async fn stale_scheduler_scope_does_not_activate_same_name_replacement() {
    use crate::backend::entities::project::Project;

    let (temp, store) = test_store().await;
    let old_project_id = projects::project_id(&store, "demo").await.unwrap();
    Project::delete_by_id(old_project_id)
        .exec(store.db().as_ref())
        .await
        .unwrap();
    let replacement = create_project(
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
    let trigger = create_trigger(
        &store,
        "demo",
        CreateAutomationTrigger {
            name: "replacement-review".to_owned(),
            enabled: true,
            activation: AutomationActivation::Manual,
            effect: AutomationEffect::ProduceWork,
            schedule: "@every 15s".to_owned(),
            tool_name: None,
            mutability: AutomationRunMutability::Mutating,
            personality_id: None,
            prompt: "Review work in the replacement project.".to_owned(),
            work_item_selector: None,
            priority: 100,
        },
    )
    .await
    .unwrap();
    schedule_trigger_evaluation(&store, "demo", trigger.id)
        .await
        .unwrap();

    let stale_project_ids = [old_project_id];
    let outcomes = run_due_triggers_with_sessions_for_projects(
        &store,
        None,
        None,
        Some(&stale_project_ids),
        None,
    )
    .await
    .unwrap();
    let trigger = list_triggers(&store, "demo")
        .await
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == trigger.id)
        .unwrap();

    assert_that!(&(replacement.id)).is_not_equal_to(old_project_id);
    assert_that!(&(outcomes.is_empty())).is_true();
    assert_that!(&(trigger.pending_evaluation_count)).is_equal_to(1);
    assert_that!(&(trigger.evaluation_count)).is_equal_to(0);
}

#[tokio::test]
async fn rule_policy_validation_agrees_for_operator_bundle_and_storage_inputs() {
    use crate::backend::automation_bundles;
    use serde_json::json;

    let (_temp, store) = test_store().await;
    let original = list_triggers(&store, "demo").await.unwrap().remove(0);
    let model = AutomationTrigger::find_by_id(original.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    let cases = [
        (
            "consume_work",
            json!({"produced_work": {}}),
            "only valid for produce_work",
        ),
        (
            "produce_work",
            json!({"postconditions": {"any_of": [{}]}}),
            "only valid for consume_work",
        ),
        (
            "consume_work",
            json!({"execution": {"model": "unknown-model"}}),
            "automation model override must be one of",
        ),
        (
            "consume_work",
            json!({"execution": {"timeout_seconds": 0}}),
            "timeout must be positive",
        ),
        (
            "consume_work",
            json!({"execution": {"max_concurrent_runs": 0}}),
            "concurrent-run limit must be positive",
        ),
        (
            "consume_work",
            json!({"execution": {"concurrency_group": "Invalid Group"}}),
            "concurrency group must use lowercase",
        ),
        (
            "consume_work",
            json!({"postconditions": {"any_of": []}}),
            "any_of cannot be empty",
        ),
        (
            "consume_work",
            json!({"postconditions": {"any_of": [{"created_items": {"count": 1, "at_least": 2}}]}}),
            "count cannot be combined",
        ),
        (
            "produce_work",
            json!({"produced_work": {"state": " "}}),
            "state label value cannot be empty",
        ),
        (
            "produce_work",
            json!({"produced_work": {"initial_labels": [{"key": "state", "value": "open"}]}}),
            "use the state selector",
        ),
        (
            "produce_work",
            json!({"produced_work": {"deduplication": {"policy": "while_unfinished_for_key", "key": "Invalid Key"}}}),
            "deduplication key must use lowercase",
        ),
    ];
    for (effect, fields, expected) in cases {
        let mut input = json!({
            "key": "policy", "name": "Policy validation", "enabled": true,
            "activation": "manual", "effect": effect, "schedule": "15s",
            "prompt_markdown": "Produce or consume work",
            "selector": if effect == "consume_work" { Some(open_state_selector()) } else { None },
            "personality": if effect == "consume_work" { Some("Default") } else { None },
        });
        input
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let input: AutomationRuleInput = serde_json::from_value(input).unwrap();

        let error = create_trigger_from_input(&store, "demo", input.clone())
            .await
            .unwrap_err();
        assert_that!(&error.to_string()).contains(expected);
        let error = update_trigger_from_input(&store, "demo", original.id, input.clone())
            .await
            .unwrap_err();
        assert_that!(&error.to_string()).contains(expected);

        let mut bundle_rule = input.clone();
        bundle_rule.personality = (effect == "consume_work").then(|| "default".to_owned());
        let manifest = json!({
            "schema_version": 1, "bundle_key": "test", "display_name": "Test",
            "personalities": [{"key": "default", "name": "Default", "description": ""}],
            "automations": [bundle_rule],
        });
        let error = automation_bundles::validate_yaml(&yaml_serde::to_string(&manifest).unwrap())
            .unwrap_err();
        assert_that!(&error.to_string()).contains(expected);

        let mut stored = model.clone();
        stored.effect = effect.to_owned();
        stored.produced_work_spec_json = input
            .produced_work
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .unwrap();
        stored.postconditions_json = input
            .postconditions
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .unwrap();
        stored.model_override = input.execution.model;
        stored.timeout_seconds = input.execution.timeout_seconds.map(|value| value as i64);
        stored.max_concurrent_runs = input
            .execution
            .max_concurrent_runs
            .map(|value| value as i64);
        stored.concurrency_group = input.execution.concurrency_group;
        let error = model_to_view(stored).unwrap_err();
        assert_that!(&error.to_string()).contains(expected);
        assert_that!(&error.to_string()).contains(format!("automation trigger {}", original.id));
    }
    let after = AutomationTrigger::find_by_id(original.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_that!(&after).is_equal_to(model);
    assert_that!(&list_triggers(&store, "demo").await.unwrap().len()).is_equal_to(3);
}

#[tokio::test]
async fn legacy_effect_updates_preserve_rules_with_incompatible_retained_policy() {
    let (_temp, store) = test_store().await;
    let original = list_triggers(&store, "demo").await.unwrap().remove(0);
    let existing = AutomationTrigger::find_by_id(original.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    let mut active: AutomationTriggerActiveModel = existing.into();
    active.postconditions_json = Set(Some(r#"{"any_of":[{}]}"#.to_owned()));
    let existing = active.update(store.db().as_ref()).await.unwrap();
    let error = update_trigger(
        &store,
        "demo",
        original.id,
        UpdateAutomationTrigger {
            name: original.name,
            enabled: true,
            activation: AutomationActivation::Manual,
            effect: AutomationEffect::ProduceWork,
            schedule: "15s".to_owned(),
            mutability: AutomationRunMutability::Mutating,
            personality_id: None,
            prompt: "Produce work".to_owned(),
            work_item_selector: None,
            priority: None,
        },
    )
    .await
    .unwrap_err();
    assert_that!(&error.to_string()).contains("only valid for consume_work");
    assert_that!(
        &AutomationTrigger::find_by_id(original.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap()
    )
    .is_equal_to(existing);
}

#[tokio::test]
async fn invalid_stored_policy_cannot_be_read_or_queued() {
    let (_temp, store) = test_store().await;
    let original = list_triggers(&store, "demo").await.unwrap().remove(0);
    let existing = AutomationTrigger::find_by_id(original.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    for timeout in [0, -1] {
        let mut active: AutomationTriggerActiveModel = existing.clone().into();
        active.timeout_seconds = Set(Some(timeout));
        let invalid = active.update(store.db().as_ref()).await.unwrap();
        let error = get_trigger(&store, "demo", &original.id.to_string())
            .await
            .unwrap_err();
        assert_that!(&error.to_string()).contains("timeout must be positive");
        assert_that!(&error.to_string()).contains(format!("automation trigger {}", original.id));
        let error = schedule_trigger_evaluation(&store, "demo", original.id)
            .await
            .unwrap_err();
        assert_that!(&error.to_string()).contains("timeout must be positive");
        assert_that!(
            &AutomationTrigger::find_by_id(original.id)
                .one(store.db().as_ref())
                .await
                .unwrap()
                .unwrap()
        )
        .is_equal_to(invalid);
    }
}
