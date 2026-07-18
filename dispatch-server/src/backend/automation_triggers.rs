use std::{collections::HashMap, str::FromStr, time::Duration as StdDuration};

use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::watch;

use crate::{
    backend::{
        automation::{self, AutomationTriggerOrigin, StartAutomation},
        automation_admission,
        automation_controller::AutomationController,
        automation_revisions::{self, RevisionActor},
        codex_app_server::SharedCodexStatus,
        entities::{
            automation_evaluation::AutomationEvaluationActiveModel,
            automation_trigger::{
                self, AutomationTrigger, AutomationTriggerActiveModel, AutomationTriggerModel,
            },
            work_item::{self, WorkItem},
            work_item_event,
            work_item_origin::{self, WorkItemOrigin},
        },
        events, item_claims,
        items::{self, CreateWorkItem},
        label_conditions, personalities,
        process_sessions::ProcessSessionRegistry,
        projects,
        storage::{Store, utc_now},
        work_item_creation::{self, CreateWorkItemPlan, InsertWorkItemOrigin},
        work_item_events,
    },
    shared::view_models::{
        AgentReasoningEffort, AgentToolName, AutomationActivation, AutomationEffect,
        AutomationEvaluationOutcome, AutomationExecutionPolicy, AutomationPostconditions,
        AutomationRuleInput, AutomationRunMutability, AutomationTriggerView, ProduceDeduplication,
        ProducedWorkSpec, RevisionChangeOperation, TriggerRunOutcome, WorkItemOriginKind,
        default_automation_work_item_selector, needs_refinement_automation_work_item_selector,
        needs_verification_automation_work_item_selector,
    },
};

const DEFAULT_WORK_ITEM_AUTOMATION_NAME: &str = "Claim open work";
const DEFAULT_REFINEMENT_AUTOMATION_NAME: &str = "Refine needs-refinement work";
const DEFAULT_VERIFICATION_AUTOMATION_NAME: &str = "Verify needs-verification work";
const DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE: &str = "@every 15s";
const SCHEDULER_TICK_SECONDS: u64 = 1;
const MAINTENANCE_TICK_SECONDS: u64 = 15;
const PRIORITY_SCORE_SECONDS: i64 = 300;
const EVALUATION_COUNT_SCORE_SECONDS: i64 = 300;
const NEVER_RUN_SCORE_SECONDS: i64 = 24 * 60 * 60;
const REFINEMENT_AUTOMATION_PRIORITY: i64 = 20;
const VERIFICATION_AUTOMATION_PRIORITY: i64 = 10;

const DEFAULT_REFINEMENT_AUTOMATION_PROMPT: &str = r#"You are the needs-refinement executor for the claimed Dispatch work item.

Goal: turn a rough or under-specified item into implementation-ready work. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory before editing it.
- Clarify the title and description so a later implementation agent can act without guessing. Prefer concrete scope, non-goals, acceptance criteria, suggested approach, verification expectations, and open questions only when human input is genuinely required.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-refinement` label when refinement is complete. Keep or add `needs-verification` only when the refined item should be checked before implementation.
- Add a concise progress comment summarizing what changed.

Do not call `dispatch item finish` for successful refinement, and do not call `dispatch item release` after successful refinement. Let Dispatch release the temporary claim after your final response. If the item cannot be refined without a human decision, leave `needs-refinement` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user."#;

const DEFAULT_VERIFICATION_AUTOMATION_PROMPT: &str = r#"You are the needs-verification executor for the claimed Dispatch work item.

Goal: verify whether the item is necessary, accurate, and ready for a later implementation agent. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory. Inspect repository files only as needed to verify facts.
- Update the title or description with verification findings, corrected scope, risks, acceptance criteria, and verification notes that future workers need.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-verification` label when verification is complete. Add `needs-refinement` only if the item still needs story-shaping before implementation.
- Add a concise progress comment with the verification result.

If verification shows the work is unnecessary, explain why in the item and a comment. You may move the item to a project-specific terminal state only when that state already exists in the project's visible workflow vocabulary; do not invent or hardcode a state name. Use `dispatch label suggestions --json`, existing item labels, comments, or project docs to infer that vocabulary.

Do not call `dispatch item finish` for successful verification, and do not call `dispatch item release` after successful verification. Let Dispatch release the temporary claim after your final response. If verification needs a user decision, leave `needs-verification` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user. If verification is blocked by a technical or environment issue rather than missing user input, call `dispatch item release --comment ...` with the blocker."#;

struct DefaultProjectAutomation {
    name: &'static str,
    prompt: &'static str,
    selector: fn() -> Condition,
    priority: i64,
    mutability: AutomationRunMutability,
}

#[derive(Clone, Debug)]
pub struct CreateAutomationTrigger {
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub tool_name: Option<AgentToolName>,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<Condition>,
    pub priority: i64,
}

#[derive(Clone, Debug)]
pub struct UpdateAutomationTrigger {
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<Condition>,
    pub priority: Option<i64>,
}

pub async fn list_triggers(
    store: &Store,
    project_name: &str,
) -> Result<Vec<AutomationTriggerView>> {
    let project_id = projects::project_id(store, project_name).await?;
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .order_by_asc(automation_trigger::Column::Name)
        .all(store.db().as_ref())
        .await
        .context("failed to list automation triggers")?;
    triggers.into_iter().map(model_to_view).collect()
}

pub async fn get_trigger(
    store: &Store,
    project_name: &str,
    id_or_key: &str,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let query =
        AutomationTrigger::find().filter(automation_trigger::Column::ProjectId.eq(project_id));
    let query = match id_or_key.parse::<i64>() {
        Ok(id) => query.filter(automation_trigger::Column::Id.eq(id)),
        Err(_) => query.filter(
            sea_orm::Condition::any()
                .add(automation_trigger::Column::ManagedObjectKey.eq(id_or_key))
                .add(automation_trigger::Column::Name.eq(id_or_key)),
        ),
    };
    let trigger = query
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| {
            report!("automation trigger '{id_or_key}' does not exist in this project")
        })?;
    model_to_view(trigger)
}

pub async fn create_trigger(
    store: &Store,
    project_name: &str,
    create: CreateAutomationTrigger,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let now = utc_now();
    let schedule = normalize_schedule(create.schedule)?;
    let work_item_selector = selector_for_activation(create.activation, create.work_item_selector)?;
    validate_trigger_configuration(
        &create.name,
        create.activation,
        create.effect,
        &schedule,
        work_item_selector.as_ref(),
        &create.prompt,
    )?;
    let personality_id =
        personality_id_for_effect(store, project_id, create.effect, create.personality_id).await?;
    let next_evaluation_at = match create.activation {
        AutomationActivation::Manual => None,
        AutomationActivation::WorkItem => None,
        AutomationActivation::Cron => Some(next_evaluation_at(&schedule)?),
        AutomationActivation::WorkItemCreated => None,
    };
    let last_event_id = match create.activation {
        AutomationActivation::Manual
        | AutomationActivation::WorkItem
        | AutomationActivation::Cron => None,
        AutomationActivation::WorkItemCreated => {
            latest_item_created_event_id(store, project_id).await?
        }
    };
    let default_tool = crate::backend::projects::get_settings(store, project_name)
        .await?
        .default_agent_tool;
    let tool_name = create.tool_name.unwrap_or(default_tool);

    let trigger = AutomationTriggerActiveModel {
        project_id: Set(project_id),
        name: Set(create.name),
        enabled: Set(create.enabled),
        activation: Set(create.activation.as_storage().to_owned()),
        effect: Set(create.effect.as_storage().to_owned()),
        schedule: Set(schedule),
        tool_name: Set(tool_name.as_storage().to_owned()),
        mutability: Set(create.mutability.as_storage().to_owned()),
        personality_id: Set(personality_id),
        prompt: Set(create.prompt),
        work_item_selector: Set(selector_to_storage(work_item_selector.as_ref())?),
        priority: Set(create.priority),
        evaluation_count: Set(0),
        pending_evaluation_count: Set(0),
        last_evaluation_queued_at: Set(None),
        last_evaluated_at: Set(None),
        next_evaluation_at: Set(next_evaluation_at),
        last_event_id: Set(last_event_id),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .context("failed to create automation trigger")?;

    automation_revisions::record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Create,
        &RevisionActor::default(),
    )
    .await?;
    let trigger = AutomationTrigger::find_by_id(trigger.id)
        .one(store.db().as_ref())
        .await
        .context("failed to reload created automation trigger")?
        .ok_or_else(|| report!("created automation trigger disappeared"))?;

    events::publish_automation_changed(project_name);
    model_to_view(trigger)
}

pub async fn create_trigger_from_input(
    store: &Store,
    project_name: &str,
    mut input: AutomationRuleInput,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    ensure_trigger_name_available(store, project_id, &input.name, None).await?;
    let schedule = normalize_schedule(input.schedule.clone())?;
    let selector = selector_for_activation(input.activation, input.selector.take())?;
    validate_rule_input(store, project_id, &input, &schedule, selector.as_ref()).await?;
    let personality_id = resolve_personality_reference(
        store,
        project_id,
        input.effect,
        input.personality.as_deref(),
    )
    .await?;
    let now = utc_now();
    let trigger = AutomationTriggerActiveModel {
        project_id: Set(project_id),
        name: Set(input.name),
        enabled: Set(input.enabled),
        activation: Set(input.activation.as_storage().to_owned()),
        effect: Set(input.effect.as_storage().to_owned()),
        schedule: Set(schedule.clone()),
        tool_name: Set(input.tool_name.as_storage().to_owned()),
        mutability: Set(input.mutability.as_storage().to_owned()),
        personality_id: Set(personality_id),
        prompt: Set(crate::backend::automation_bundles::markdown_to_html(
            &input.prompt_markdown,
        )),
        work_item_selector: Set(selector_to_storage(selector.as_ref())?),
        priority: Set(input.priority),
        exclusive: Set(input.exclusive),
        produced_work_spec_json: Set(input
            .produced_work
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?),
        postconditions_json: Set(input
            .postconditions
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?),
        model_override: Set(input.execution.model),
        reasoning_effort_override: Set(input
            .execution
            .reasoning_effort
            .map(|value| value.as_storage().to_owned())),
        timeout_seconds: Set(input.execution.timeout_seconds.map(|value| value as i64)),
        max_concurrent_runs: Set(input
            .execution
            .max_concurrent_runs
            .map(|value| value as i64)),
        concurrency_group: Set(input.execution.concurrency_group),
        current_revision_id: Set(None),
        managed_bundle_key: Set(None),
        managed_object_key: Set(None),
        evaluation_count: Set(0),
        pending_evaluation_count: Set(0),
        last_evaluation_queued_at: Set(None),
        last_evaluated_at: Set(None),
        next_evaluation_at: Set(match input.activation {
            AutomationActivation::Cron => Some(next_evaluation_at(&schedule)?),
            _ => None,
        }),
        last_event_id: Set(match input.activation {
            AutomationActivation::WorkItemCreated => {
                latest_item_created_event_id(store, project_id).await?
            }
            _ => None,
        }),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .context("failed to create automation trigger")?;
    automation_revisions::record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Create,
        &RevisionActor::default(),
    )
    .await?;
    events::publish_automation_changed(project_name);
    get_trigger(store, project_name, &trigger.id.to_string()).await
}

pub async fn update_trigger_from_input(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
    mut input: AutomationRuleInput,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let existing = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    if existing.managed_bundle_key.is_some() {
        bail!("bundle-managed automations must be detached before individual editing");
    }
    ensure_trigger_name_available(store, project_id, &input.name, Some(trigger_id)).await?;
    let schedule = normalize_schedule(input.schedule.clone())?;
    let selector = selector_for_activation(input.activation, input.selector.take())?;
    validate_rule_input(store, project_id, &input, &schedule, selector.as_ref()).await?;
    let personality_id = resolve_personality_reference(
        store,
        project_id,
        input.effect,
        input.personality.as_deref(),
    )
    .await?;
    let mut active: AutomationTriggerActiveModel = existing.into();
    active.name = Set(input.name);
    active.enabled = Set(input.enabled);
    active.activation = Set(input.activation.as_storage().to_owned());
    active.effect = Set(input.effect.as_storage().to_owned());
    active.schedule = Set(schedule.clone());
    active.tool_name = Set(input.tool_name.as_storage().to_owned());
    active.mutability = Set(input.mutability.as_storage().to_owned());
    active.personality_id = Set(personality_id);
    active.prompt = Set(crate::backend::automation_bundles::markdown_to_html(
        &input.prompt_markdown,
    ));
    active.work_item_selector = Set(selector_to_storage(selector.as_ref())?);
    active.priority = Set(input.priority);
    active.exclusive = Set(input.exclusive);
    active.produced_work_spec_json = Set(input
        .produced_work
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?);
    active.postconditions_json = Set(input
        .postconditions
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?);
    active.model_override = Set(input.execution.model);
    active.reasoning_effort_override = Set(input
        .execution
        .reasoning_effort
        .map(|value| value.as_storage().to_owned()));
    active.timeout_seconds = Set(input.execution.timeout_seconds.map(|value| value as i64));
    active.max_concurrent_runs = Set(input
        .execution
        .max_concurrent_runs
        .map(|value| value as i64));
    active.concurrency_group = Set(input.execution.concurrency_group);
    active.next_evaluation_at = Set(match input.activation {
        AutomationActivation::Cron => Some(next_evaluation_at(&schedule)?),
        _ => None,
    });
    active.updated_at = Set(utc_now());
    let trigger = active
        .update(store.db().as_ref())
        .await
        .context("failed to update automation trigger")?;
    automation_revisions::record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Update,
        &RevisionActor::default(),
    )
    .await?;
    events::publish_automation_changed(project_name);
    get_trigger(store, project_name, &trigger_id.to_string()).await
}

async fn validate_rule_input(
    store: &Store,
    project_id: i64,
    input: &AutomationRuleInput,
    schedule: &str,
    selector: Option<&Condition>,
) -> Result<()> {
    validate_trigger_configuration(
        &input.name,
        input.activation,
        input.effect,
        schedule,
        selector,
        &input.prompt_markdown,
    )?;
    automation_admission::validate_execution_policy(&input.execution)?;
    projects::validate_agent_model_reasoning_effort(
        "automation model override",
        input.execution.model.as_deref(),
        "automation reasoning-effort override",
        input.execution.reasoning_effort,
    )?;
    if let Some(spec) = &input.produced_work {
        validate_produced_work_spec(spec)?;
    }
    if input.effect == AutomationEffect::ProduceWork && input.postconditions.is_some() {
        bail!("postconditions are only valid for consume_work automations");
    }
    if input
        .postconditions
        .as_ref()
        .is_some_and(|value| value.any_of.is_empty())
    {
        bail!("automation postconditions require at least one outcome set");
    }
    if let Some(postconditions) = input.postconditions.as_ref() {
        crate::backend::automation_bundles::validate_postcondition_configuration(
            postconditions,
            "postconditions",
        )?;
    }
    if input.effect == AutomationEffect::ConsumeWork {
        resolve_personality_reference(
            store,
            project_id,
            input.effect,
            input.personality.as_deref(),
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_extended_trigger_configuration(
    effect: AutomationEffect,
    produced_work_spec_json: Option<&str>,
    postconditions_json: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    timeout_seconds: Option<i64>,
    max_concurrent_runs: Option<i64>,
    concurrency_group: Option<&str>,
) -> Result<()> {
    let produced_work = produced_work_spec_json
        .map(serde_json::from_str::<ProducedWorkSpec>)
        .transpose()
        .context("invalid produced-work specification JSON")?;
    let postconditions = postconditions_json
        .map(serde_json::from_str::<AutomationPostconditions>)
        .transpose()
        .context("invalid semantic postconditions JSON")?;
    if effect == AutomationEffect::ConsumeWork && produced_work.is_some() {
        bail!("produced-work specification is only valid for produce_work automations");
    }
    if effect == AutomationEffect::ProduceWork && postconditions.is_some() {
        bail!("postconditions are only valid for consume_work automations");
    }
    if let Some(spec) = produced_work.as_ref() {
        validate_produced_work_spec(spec)?;
    }
    if let Some(postconditions) = postconditions.as_ref() {
        crate::backend::automation_bundles::validate_postcondition_configuration(
            postconditions,
            "postconditions",
        )?;
    }
    let reasoning_effort = reasoning_effort
        .map(str::parse::<AgentReasoningEffort>)
        .transpose()?;
    let execution = AutomationExecutionPolicy {
        model: model.map(str::to_owned),
        reasoning_effort,
        timeout_seconds: timeout_seconds
            .map(|value| u64::try_from(value).context("automation timeout must be positive"))
            .transpose()?,
        max_concurrent_runs: max_concurrent_runs
            .map(|value| {
                u64::try_from(value).context("automation concurrent-run limit must be positive")
            })
            .transpose()?,
        concurrency_group: concurrency_group.map(str::to_owned),
    };
    automation_admission::validate_execution_policy(&execution)?;
    projects::validate_agent_model_reasoning_effort(
        "automation model override",
        execution.model.as_deref(),
        "automation reasoning-effort override",
        execution.reasoning_effort,
    )
}

async fn resolve_personality_reference(
    store: &Store,
    project_id: i64,
    effect: AutomationEffect,
    reference: Option<&str>,
) -> Result<Option<i64>> {
    if effect != AutomationEffect::ConsumeWork {
        return Ok(None);
    }
    let Some(reference) = reference else {
        return Ok(Some(
            personalities::default_personality_id(store, project_id).await?,
        ));
    };
    let personality = crate::backend::entities::personality::Personality::find()
        .filter(crate::backend::entities::personality::Column::ProjectId.eq(project_id))
        .filter(
            sea_orm::Condition::any()
                .add(crate::backend::entities::personality::Column::ManagedObjectKey.eq(reference))
                .add(crate::backend::entities::personality::Column::Name.eq(reference)),
        )
        .one(store.db().as_ref())
        .await
        .context("failed to resolve automation personality")?
        .ok_or_else(|| report!("personality '{reference}' does not exist in this project"))?;
    Ok(Some(personality.id))
}

async fn ensure_trigger_name_available(
    store: &Store,
    project_id: i64,
    name: &str,
    except_id: Option<i64>,
) -> Result<()> {
    let mut query = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::Name.eq(name));
    if let Some(except_id) = except_id {
        query = query.filter(automation_trigger::Column::Id.ne(except_id));
    }
    if query.one(store.db().as_ref()).await?.is_some() {
        bail!("automation trigger name '{name}' already exists in this project");
    }
    Ok(())
}

pub async fn delete_trigger(store: &Store, project_name: &str, trigger_id: i64) -> Result<()> {
    let project_id = projects::project_id(store, project_name).await?;
    let trigger = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    if trigger.managed_bundle_key.is_some() {
        bail!("bundle-managed automations cannot be deleted individually");
    }
    AutomationTrigger::delete_by_id(trigger.id)
        .exec(store.db().as_ref())
        .await
        .context("failed to delete automation trigger")?;
    events::publish_automation_changed(project_name);
    Ok(())
}

pub async fn detach_trigger(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let existing = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    if existing.managed_bundle_key.is_none() {
        bail!("automation trigger is not bundle-managed");
    }
    let mut active: AutomationTriggerActiveModel = existing.into();
    active.managed_bundle_key = Set(None);
    active.managed_object_key = Set(None);
    active.updated_at = Set(utc_now());
    let trigger = active
        .update(store.db().as_ref())
        .await
        .context("failed to detach automation trigger")?;
    automation_revisions::record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Detach,
        &RevisionActor::default(),
    )
    .await?;
    events::publish_automation_changed(project_name);
    get_trigger(store, project_name, &trigger_id.to_string()).await
}

pub async fn update_trigger(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
    update: UpdateAutomationTrigger,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let existing = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    let previous_kind = AutomationActivation::from_str(&existing.activation)?;
    let schedule = normalize_schedule(update.schedule)?;
    let work_item_selector = selector_for_activation(update.activation, update.work_item_selector)?;
    validate_trigger_configuration(
        &update.name,
        update.activation,
        update.effect,
        &schedule,
        work_item_selector.as_ref(),
        &update.prompt,
    )?;
    let personality_id =
        personality_id_for_effect(store, project_id, update.effect, update.personality_id).await?;
    let now = utc_now();
    let next_evaluation_at = match update.activation {
        AutomationActivation::Manual => None,
        AutomationActivation::WorkItem => None,
        AutomationActivation::Cron => Some(next_evaluation_at(&schedule)?),
        AutomationActivation::WorkItemCreated => None,
    };
    let last_event_id = match (previous_kind, update.activation) {
        (AutomationActivation::WorkItemCreated, AutomationActivation::WorkItemCreated) => {
            existing.last_event_id
        }
        (_, AutomationActivation::WorkItemCreated) => {
            latest_item_created_event_id(store, project_id).await?
        }
        (
            _,
            AutomationActivation::Manual
            | AutomationActivation::WorkItem
            | AutomationActivation::Cron,
        ) => None,
    };
    let mut active: AutomationTriggerActiveModel = existing.into();
    active.name = Set(update.name);
    active.enabled = Set(update.enabled);
    active.activation = Set(update.activation.as_storage().to_owned());
    active.effect = Set(update.effect.as_storage().to_owned());
    active.schedule = Set(schedule);
    active.mutability = Set(update.mutability.as_storage().to_owned());
    active.personality_id = Set(personality_id);
    active.prompt = Set(update.prompt);
    active.work_item_selector = Set(selector_to_storage(work_item_selector.as_ref())?);
    if let Some(priority) = update.priority {
        active.priority = Set(priority);
    }
    active.next_evaluation_at = Set(next_evaluation_at);
    active.last_event_id = Set(last_event_id);
    active.updated_at = Set(now);

    let trigger = active
        .update(store.db().as_ref())
        .await
        .context("failed to update automation trigger")?;
    automation_revisions::record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Update,
        &RevisionActor::default(),
    )
    .await?;
    let trigger = AutomationTrigger::find_by_id(trigger.id)
        .one(store.db().as_ref())
        .await
        .context("failed to reload updated automation trigger")?
        .ok_or_else(|| report!("updated automation trigger disappeared"))?;
    events::publish_automation_changed(project_name);
    model_to_view(trigger)
}

#[cfg(test)]
pub async fn run_due_triggers(store: &Store) -> Result<Vec<TriggerRunOutcome>> {
    run_due_triggers_with_sessions(store, None).await
}

#[cfg(test)]
pub async fn run_due_triggers_with_sessions(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
) -> Result<Vec<TriggerRunOutcome>> {
    run_due_triggers_with_sessions_for_projects(store, sessions, None, None, None).await
}

async fn run_due_triggers_with_sessions_for_projects(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    active_project_names: Option<&[String]>,
    project_cancellations: Option<&HashMap<String, watch::Receiver<bool>>>,
) -> Result<Vec<TriggerRunOutcome>> {
    let scope = AutomationProjectScope::new(active_project_names, project_cancellations);
    let mut outcomes =
        run_queued_evaluations(store, sessions.clone(), codex_status.clone(), scope).await?;
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::Enabled.eq(true))
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load enabled automation triggers")?;

    for trigger in triggers {
        let view = model_to_view(trigger.clone())?;
        if matches!(
            view.activation,
            AutomationActivation::Manual | AutomationActivation::WorkItem
        ) {
            continue;
        }
        let project_name = projects::project_name_by_id(store, view.project_id).await?;
        if !scope.includes_project(&project_name) {
            continue;
        }
        match view.activation {
            AutomationActivation::Manual => {}
            AutomationActivation::WorkItem => {}
            AutomationActivation::Cron => {
                if trigger_is_due(view.next_evaluation_at.as_deref())
                    && let Some(outcome) = evaluate_trigger_once(
                        store,
                        &project_name,
                        trigger,
                        None,
                        sessions.clone(),
                        codex_status.clone(),
                        scope.cancellation_for(&project_name),
                    )
                    .await
                {
                    outcomes.push(outcome);
                }
            }
            AutomationActivation::WorkItemCreated => {
                let events =
                    new_item_created_events(store, view.project_id, view.last_event_id).await?;
                let mut last_event_id = view.last_event_id;
                for event in events {
                    last_event_id = Some(event.id);
                    if let Some(outcome) = evaluate_trigger_once(
                        store,
                        &project_name,
                        trigger.clone(),
                        event.work_item_id,
                        sessions.clone(),
                        codex_status.clone(),
                        scope.cancellation_for(&project_name),
                    )
                    .await
                    {
                        outcomes.push(outcome);
                    }
                }
                if last_event_id != view.last_event_id {
                    update_trigger_event_cursor(store, trigger, last_event_id).await?;
                }
            }
        }
    }
    if let Some(active_project_names) = scope.active_project_names() {
        for project_name in active_project_names {
            if let Some(outcome) = run_next_work_item_automation_for_project(
                store,
                project_name,
                sessions.clone(),
                codex_status.clone(),
                scope.cancellation_for(project_name),
            )
            .await?
            {
                outcomes.push(outcome);
            }
        }
    }
    Ok(outcomes)
}

#[derive(Clone, Copy)]
struct AutomationProjectScope<'a> {
    active_project_names: Option<&'a [String]>,
    project_cancellations: Option<&'a HashMap<String, watch::Receiver<bool>>>,
}

impl<'a> AutomationProjectScope<'a> {
    fn new(
        active_project_names: Option<&'a [String]>,
        project_cancellations: Option<&'a HashMap<String, watch::Receiver<bool>>>,
    ) -> Self {
        Self {
            active_project_names,
            project_cancellations,
        }
    }

    fn includes_project(&self, project_name: &str) -> bool {
        match self.active_project_names {
            Some(active_project_names) => active_project_names
                .iter()
                .any(|active_project_name| active_project_name == project_name),
            None => true,
        }
    }

    fn active_project_names(&self) -> Option<&'a [String]> {
        self.active_project_names
    }

    fn cancellation_for(&self, project_name: &str) -> Option<watch::Receiver<bool>> {
        self.project_cancellations
            .and_then(|cancellations| cancellations.get(project_name))
            .cloned()
    }
}

pub async fn schedule_trigger_evaluation(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let trigger = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    let now = utc_now();
    let pending_evaluation_count = trigger.pending_evaluation_count.saturating_add(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.pending_evaluation_count = Set(pending_evaluation_count);
    active.last_evaluation_queued_at = Set(Some(now.clone()));
    active.updated_at = Set(now);
    let updated = active
        .update(store.db().as_ref())
        .await
        .context("failed to queue automation evaluation")?;
    events::publish_automation_changed(project_name);
    model_to_view(updated)
}

async fn run_queued_evaluations(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    scope: AutomationProjectScope<'_>,
) -> Result<Vec<TriggerRunOutcome>> {
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::PendingEvaluationCount.gt(0))
        .order_by_desc(automation_trigger::Column::Priority)
        .order_by_asc(automation_trigger::Column::LastEvaluationQueuedAt)
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load queued automation evaluations")?;

    let mut outcomes = Vec::new();
    for trigger in triggers {
        let project_name = projects::project_name_by_id(store, trigger.project_id).await?;
        if !scope.includes_project(&project_name) {
            continue;
        }
        let view = model_to_view(trigger.clone())?;
        if view.effect == AutomationEffect::ConsumeWork {
            let settings = projects::get_settings(store, &project_name).await?;
            if automation_admission::enforce_rule_start_allowed(
                store,
                &project_name,
                &settings,
                view.mutability,
                Some(view.id),
                &view.execution,
            )
            .await
            .is_err()
            {
                continue;
            }
        }
        let trigger = consume_queued_evaluation(store, trigger).await?;
        if let Some(outcome) = evaluate_trigger_once(
            store,
            &project_name,
            trigger,
            None,
            sessions.clone(),
            codex_status.clone(),
            scope.cancellation_for(&project_name),
        )
        .await
        {
            outcomes.push(outcome);
        }
    }
    Ok(outcomes)
}

async fn consume_queued_evaluation(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let pending_evaluation_count = trigger.pending_evaluation_count.saturating_sub(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.pending_evaluation_count = Set(pending_evaluation_count);
    active.updated_at = Set(utc_now());
    Ok(active
        .update(store.db().as_ref())
        .await
        .context("failed to consume queued automation evaluation")?)
}

async fn run_next_work_item_automation_for_project(
    store: &Store,
    project_name: &str,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Result<Option<TriggerRunOutcome>> {
    let project_id = projects::project_id(store, project_name).await?;
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::Enabled.eq(true))
        .filter(
            automation_trigger::Column::Activation.eq(AutomationActivation::WorkItem.as_storage()),
        )
        .filter(automation_trigger::Column::Effect.eq(AutomationEffect::ConsumeWork.as_storage()))
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load work-item automation entries")?;

    let mut candidates = Vec::new();
    let mut checked_without_match = Vec::new();
    for trigger in triggers {
        let view = model_to_view(trigger.clone())?;
        if !trigger_is_due(view.next_evaluation_at.as_deref()) {
            continue;
        }
        let settings = projects::get_settings(store, project_name).await?;
        if automation_admission::enforce_rule_start_allowed(
            store,
            project_name,
            &settings,
            view.mutability,
            Some(view.id),
            &view.execution,
        )
        .await
        .is_err()
        {
            continue;
        }
        let Some(selector) = view.work_item_selector.as_ref() else {
            checked_without_match.push(trigger);
            continue;
        };
        let item_ids = matching_claimable_item_ids(store, project_name, selector).await?;
        if !item_ids.is_empty() {
            candidates.push(WorkItemAutomationCandidate {
                trigger,
                view,
                item_ids,
            });
        } else {
            checked_without_match.push(trigger);
        }
    }

    let Some(max_evaluation_count) = candidates
        .iter()
        .map(|candidate| candidate.view.evaluation_count)
        .max()
    else {
        for trigger in checked_without_match {
            update_trigger_after_check(store, trigger).await?;
        }
        return Ok(None);
    };
    let now = OffsetDateTime::now_utc();
    candidates.sort_by(|left, right| {
        work_item_automation_score(&right.view, max_evaluation_count, now)
            .cmp(&work_item_automation_score(
                &left.view,
                max_evaluation_count,
                now,
            ))
            .then_with(|| left.view.evaluation_count.cmp(&right.view.evaluation_count))
            .then_with(|| left.view.id.cmp(&right.view.id))
    });

    run_first_available_work_item_automation_candidate(
        store,
        project_name,
        candidates,
        sessions,
        codex_status,
        cancellation,
    )
    .await
}

struct WorkItemAutomationCandidate {
    trigger: AutomationTriggerModel,
    view: AutomationTriggerView,
    item_ids: Vec<i64>,
}

async fn run_first_available_work_item_automation_candidate(
    store: &Store,
    project_name: &str,
    candidates: Vec<WorkItemAutomationCandidate>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Result<Option<TriggerRunOutcome>> {
    let max_evaluation_count = candidates
        .iter()
        .map(|candidate| candidate.view.evaluation_count)
        .max()
        .unwrap_or_default();
    let now = OffsetDateTime::now_utc();
    let routing = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.view.id,
                candidate.view.priority,
                candidate.view.exclusive,
                work_item_automation_score(&candidate.view, max_evaluation_count, now),
                candidate.item_ids.clone(),
            )
        })
        .collect::<Vec<_>>();
    for candidate in candidates {
        for item_id in &candidate.item_ids {
            let exclusive_winner = routing
                .iter()
                .filter(|(_, _, exclusive, _, item_ids)| *exclusive && item_ids.contains(item_id))
                .max_by(|left, right| {
                    left.1
                        .cmp(&right.1)
                        .then_with(|| left.3.cmp(&right.3))
                        .then_with(|| right.0.cmp(&left.0))
                })
                .map(|entry| entry.0);
            if let Some(exclusive_winner) = exclusive_winner
                && exclusive_winner != candidate.view.id
            {
                continue;
            }
            if exclusive_winner.is_none() && candidate.view.exclusive {
                continue;
            }
            if let Some(outcome) = evaluate_trigger_once(
                store,
                project_name,
                candidate.trigger.clone(),
                Some(*item_id),
                sessions.clone(),
                codex_status.clone(),
                cancellation.clone(),
            )
            .await
            {
                return Ok(Some(outcome));
            }
        }
    }
    Ok(None)
}

async fn matching_claimable_item_ids(
    store: &Store,
    project_name: &str,
    selector: &Condition,
) -> Result<Vec<i64>> {
    let selector = label_conditions::ValidatedLabelCondition::new(selector)?;
    let mut items = items::list_items(store, project_name, None).await?;
    items.reverse();
    Ok(items
        .into_iter()
        .filter(|item| {
            item.claimed_by.is_none()
                && item.finished_at.is_none()
                && selector.matches_automation_selector(&item.labels)
        })
        .map(|item| item.id)
        .collect())
}

fn work_item_automation_score(
    automation: &AutomationTriggerView,
    max_evaluation_count: i64,
    now: OffsetDateTime,
) -> i64 {
    let age_seconds = automation
        .last_evaluated_at
        .as_deref()
        .and_then(|last_evaluated_at| OffsetDateTime::parse(last_evaluated_at, &Rfc3339).ok())
        .map(|last_evaluated_at| (now - last_evaluated_at).whole_seconds().max(0))
        .unwrap_or(NEVER_RUN_SCORE_SECONDS);
    let evaluation_count_gap = max_evaluation_count.saturating_sub(automation.evaluation_count);
    age_seconds
        .saturating_add(evaluation_count_gap.saturating_mul(EVALUATION_COUNT_SCORE_SECONDS))
        .saturating_add(automation.priority.saturating_mul(PRIORITY_SCORE_SECONDS))
}

pub fn spawn_scheduler_until(
    store: Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    controller: AutomationController,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut automation_interval =
            tokio::time::interval(StdDuration::from_secs(SCHEDULER_TICK_SECONDS));
        let mut maintenance_interval =
            tokio::time::interval(StdDuration::from_secs(MAINTENANCE_TICK_SECONDS));
        loop {
            tokio::select! {
                _ = automation_interval.tick() => {
                    let project_cancellations = controller.project_cancellations().await;
                    if !project_cancellations.is_empty() {
                        let active_projects = project_cancellations
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>();
                        if let Err(err) = run_due_triggers_with_sessions_for_projects(
                            &store,
                            sessions.clone(),
                            codex_status.clone(),
                            Some(&active_projects),
                            Some(&project_cancellations),
                        )
                        .await
                        {
                            tracing::error!(
                                error = %format_args!("{err:#}"),
                                "automation trigger scheduler failed"
                            );
                        }
                    }
                }
                _ = maintenance_interval.tick() => {
                    if let Err(err) = automation::recover_configured_stale_claims(&store).await {
                        tracing::error!(
                            error = %format_args!("{err:#}"),
                            "stale claim recovery failed"
                        );
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

async fn evaluate_trigger_once(
    store: &Store,
    project_name: &str,
    trigger: AutomationTriggerModel,
    work_item_id: Option<i64>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Option<TriggerRunOutcome> {
    let view = match model_to_view(trigger.clone()) {
        Ok(view) => view,
        Err(err) => {
            return Some(TriggerRunOutcome {
                trigger_id: trigger.id,
                trigger_name: trigger.name,
                work_item_id,
                work_item: None,
                run: None,
                error: Some(err.to_string()),
            });
        }
    };

    match view.effect {
        AutomationEffect::ProduceWork => {
            let result = create_work_item_from_trigger(store, project_name, &view).await;
            let _ = update_trigger_after_evaluation(store, trigger).await;
            let (work_item_id, work_item, error) = match result {
                Ok(work_item) => (Some(work_item.id), Some(work_item), None),
                Err(err) => {
                    let error = err.to_string();
                    let _ = insert_evaluation_in_conn(
                        store.db().as_ref(),
                        &view,
                        AutomationEvaluationOutcome::Failed,
                        None,
                        None,
                        Some(error.clone()),
                    )
                    .await;
                    (None, None, Some(error))
                }
            };
            Some(TriggerRunOutcome {
                trigger_id: view.id,
                trigger_name: view.name,
                work_item_id,
                work_item,
                run: None,
                error,
            })
        }
        AutomationEffect::ConsumeWork => {
            match trigger_has_consumable_work(store, project_name, &view, work_item_id).await {
                Ok(true) => Some(
                    run_trigger_once(
                        store,
                        project_name,
                        trigger,
                        work_item_id,
                        sessions,
                        codex_status,
                        cancellation,
                    )
                    .await,
                ),
                Ok(false) => {
                    let _ = update_trigger_after_check(store, trigger).await;
                    None
                }
                Err(err) => {
                    let _ = update_trigger_after_check(store, trigger).await;
                    Some(TriggerRunOutcome {
                        trigger_id: view.id,
                        trigger_name: view.name,
                        work_item_id,
                        work_item: None,
                        run: None,
                        error: Some(err.to_string()),
                    })
                }
            }
        }
    }
}

async fn create_work_item_from_trigger(
    store: &Store,
    project_name: &str,
    automation: &AutomationTriggerView,
) -> Result<crate::shared::view_models::WorkItemView> {
    let spec = automation
        .produced_work
        .clone()
        .unwrap_or(ProducedWorkSpec {
            title: None,
            state: crate::shared::view_models::DEFAULT_STATE_LABEL.to_owned(),
            initial_labels: Vec::new(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            deduplication: ProduceDeduplication::Always,
        });
    validate_produced_work_spec(&spec)?;
    let create = CreateWorkItemPlan::new(CreateWorkItem {
        title: spec
            .title
            .clone()
            .unwrap_or_else(|| automation.name.clone()),
        description: automation.prompt.clone(),
        state: spec.state.clone(),
        agent_model_override: spec.agent_model_override.clone(),
        agent_reasoning_effort_override: spec.agent_reasoning_effort_override,
        initial_labels: spec.initial_labels.clone(),
    })?;
    let project_id = projects::project_id(store, project_name).await?;
    let settings = projects::get_settings_by_id(store, project_id).await?;
    items::validate_effective_agent_selection(
        &settings,
        create.agent_model_override(),
        create.agent_reasoning_effort_override(),
    )?;

    let _production_guard = store.lock_automation_production().await;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start produced-work evaluation")?;
    if let Some(existing_item_id) =
        unfinished_duplicate_item_id(&txn, project_id, automation.id, &spec.deduplication).await?
    {
        insert_evaluation_in_conn(
            &txn,
            automation,
            AutomationEvaluationOutcome::SkippedDuplicate,
            Some(existing_item_id),
            None,
            None,
        )
        .await?;
        txn.commit()
            .await
            .context("failed to commit duplicate produced-work evaluation")?;
        return items::get_item(store, project_name, existing_item_id).await;
    }

    let evaluation = insert_evaluation_in_conn(
        &txn,
        automation,
        AutomationEvaluationOutcome::CreatedWork,
        None,
        None,
        None,
    )
    .await?;
    let now = utc_now();
    let item = work_item_creation::insert_planned_with_origin_in_tx(
        &txn,
        project_id,
        create,
        now,
        InsertWorkItemOrigin {
            kind: WorkItemOriginKind::ProducingAutomation,
            actor_id: None,
            agent_run_id: None,
            producing_evaluation_id: Some(evaluation.id),
            trigger_id: Some(automation.id),
            trigger_revision_id: automation.current_revision_id,
            trigger_name: Some(automation.name.clone()),
            bundle_key: automation.managed_bundle_key.clone(),
            deduplication_key: deduplication_key(&spec.deduplication),
        },
        work_item_events::EventAttribution::default(),
    )
    .await?;
    let mut evaluation_active: AutomationEvaluationActiveModel = evaluation.into();
    evaluation_active.work_item_id = Set(Some(item.id));
    evaluation_active
        .update(&txn)
        .await
        .context("failed to attach produced item to evaluation")?;
    txn.commit()
        .await
        .context("failed to commit produced-work evaluation")?;
    events::publish_work_item_changed(project_name, item.id);
    crate::backend::work_item_views::model_to_view(store, item).await
}

fn validate_produced_work_spec(spec: &ProducedWorkSpec) -> Result<()> {
    if let Some(title) = &spec.title
        && title.trim().is_empty()
    {
        bail!("produced-work title cannot be empty when configured");
    }
    if let ProduceDeduplication::WhileUnfinishedForKey { key } = &spec.deduplication {
        validate_stable_key("produced-work deduplication key", key)?;
    }
    Ok(())
}

pub(crate) fn validate_stable_key(field: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        bail!("{field} must use lowercase letters, digits, '.', '_' or '-'");
    }
    Ok(())
}

fn deduplication_key(policy: &ProduceDeduplication) -> Option<String> {
    match policy {
        ProduceDeduplication::WhileUnfinishedForKey { key } => Some(key.clone()),
        _ => None,
    }
}

async fn unfinished_duplicate_item_id<C>(
    conn: &C,
    project_id: i64,
    trigger_id: i64,
    policy: &ProduceDeduplication,
) -> Result<Option<i64>>
where
    C: ConnectionTrait,
{
    let mut query = WorkItemOrigin::find()
        .filter(work_item_origin::Column::ProjectId.eq(project_id))
        .filter(
            work_item_origin::Column::OriginKind
                .eq(WorkItemOriginKind::ProducingAutomation.as_storage()),
        )
        .order_by_desc(work_item_origin::Column::WorkItemId);
    query = match policy {
        ProduceDeduplication::Always => return Ok(None),
        ProduceDeduplication::WhileUnfinishedForTrigger => {
            query.filter(work_item_origin::Column::TriggerId.eq(trigger_id))
        }
        ProduceDeduplication::WhileUnfinishedForKey { key } => {
            query.filter(work_item_origin::Column::DeduplicationKey.eq(key))
        }
    };
    for origin in query
        .all(conn)
        .await
        .context("failed to inspect produced-work deduplication origins")?
    {
        if WorkItem::find_by_id(origin.work_item_id)
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::FinishedAt.is_null())
            .one(conn)
            .await
            .context("failed to inspect produced-work duplicate")?
            .is_some()
        {
            return Ok(Some(origin.work_item_id));
        }
    }
    Ok(None)
}

async fn insert_evaluation_in_conn<C>(
    conn: &C,
    automation: &AutomationTriggerView,
    outcome: AutomationEvaluationOutcome,
    work_item_id: Option<i64>,
    run_id: Option<i64>,
    error: Option<String>,
) -> Result<crate::backend::entities::automation_evaluation::Model>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    Ok(AutomationEvaluationActiveModel {
        project_id: Set(automation.project_id),
        trigger_id: Set(Some(automation.id)),
        trigger_revision_id: Set(automation.current_revision_id),
        trigger_name: Set(automation.name.clone()),
        activation_cause: Set(automation.activation.as_storage().to_owned()),
        outcome: Set(outcome.as_storage().to_owned()),
        work_item_id: Set(work_item_id),
        run_id: Set(run_id),
        error: Set(error),
        created_at: Set(now.clone()),
        completed_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to record automation evaluation")?)
}

async fn trigger_has_consumable_work(
    store: &Store,
    project_name: &str,
    automation: &AutomationTriggerView,
    work_item_id: Option<i64>,
) -> Result<bool> {
    let Some(selector) = automation.work_item_selector.as_ref() else {
        return Ok(false);
    };
    if let Some(work_item_id) = work_item_id {
        return item_claims::has_claimable_specific_item_matching_condition(
            store,
            project_name,
            work_item_id,
            selector,
        )
        .await;
    };
    item_claims::has_claimable_item_matching_condition(store, project_name, selector).await
}

async fn run_trigger_once(
    store: &Store,
    project_name: &str,
    trigger: AutomationTriggerModel,
    work_item_id: Option<i64>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> TriggerRunOutcome {
    let view = match model_to_view(trigger.clone()) {
        Ok(view) => view,
        Err(err) => {
            return TriggerRunOutcome {
                trigger_id: trigger.id,
                trigger_name: trigger.name,
                work_item_id,
                work_item: None,
                run: None,
                error: Some(err.to_string()),
            };
        }
    };

    let result = automation::start_automation_with_sessions_until(
        store,
        project_name,
        StartAutomation {
            tool: Some(view.tool_name),
            work_item_id,
            work_item_selector: view.work_item_selector.clone(),
            extra_prompt: Some(view.prompt.clone()),
            mutability: Some(view.mutability),
            personality_id: view.personality_id,
            trigger: Some(AutomationTriggerOrigin {
                trigger_id: view.id,
                trigger_name: view.name.clone(),
                trigger_revision_id: view.current_revision_id,
            }),
            execution: view.execution.clone(),
            postconditions: view.postconditions.clone(),
        },
        sessions,
        codex_status,
        cancellation,
    )
    .await;

    let (run, error) = match result {
        Ok(run) => (Some(run), None),
        Err(err) => (None, Some(err.to_string())),
    };
    let _ = insert_evaluation_in_conn(
        store.db().as_ref(),
        &view,
        if run.is_some() {
            AutomationEvaluationOutcome::StartedRun
        } else {
            AutomationEvaluationOutcome::Failed
        },
        run.as_ref()
            .and_then(|run| run.work_item_id)
            .or(work_item_id),
        run.as_ref().map(|run| run.id),
        error.clone(),
    )
    .await;
    let _ = update_trigger_after_run(store, trigger).await;
    let outcome_work_item_id = run
        .as_ref()
        .and_then(|run| run.work_item_id)
        .or(work_item_id);

    TriggerRunOutcome {
        trigger_id: view.id,
        trigger_name: view.name,
        work_item_id: outcome_work_item_id,
        work_item: None,
        run,
        error,
    }
}

async fn update_trigger_after_evaluation(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let view = model_to_view(trigger.clone())?;
    let now = utc_now();
    let next = match view.activation {
        AutomationActivation::WorkItem | AutomationActivation::Cron => {
            Some(next_evaluation_at(&view.schedule)?)
        }
        AutomationActivation::Manual => view.next_evaluation_at,
        AutomationActivation::WorkItemCreated => view.next_evaluation_at,
    };
    let evaluation_count = trigger.evaluation_count.saturating_add(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.last_evaluated_at = Set(Some(now.clone()));
    active.next_evaluation_at = Set(next);
    active.evaluation_count = Set(evaluation_count);
    active.updated_at = Set(now);
    let updated = active
        .update(store.db().as_ref())
        .await
        .context("failed to update automation trigger after evaluation")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn update_trigger_after_run(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    update_trigger_after_evaluation(store, trigger).await
}

async fn update_trigger_after_check(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let view = model_to_view(trigger.clone())?;
    let mut active: AutomationTriggerActiveModel = trigger.into();
    let next = match view.activation {
        AutomationActivation::WorkItem | AutomationActivation::Cron => {
            Some(next_evaluation_at(&view.schedule)?)
        }
        AutomationActivation::Manual | AutomationActivation::WorkItemCreated => {
            view.next_evaluation_at
        }
    };
    active.next_evaluation_at = Set(next);
    active.updated_at = Set(utc_now());
    let updated = active
        .update(store.db().as_ref())
        .await
        .context("failed to update automation trigger after check")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn update_trigger_event_cursor(
    store: &Store,
    trigger: AutomationTriggerModel,
    last_event_id: Option<i64>,
) -> Result<AutomationTriggerModel> {
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.last_event_id = Set(last_event_id);
    active.updated_at = Set(utc_now());
    let updated = active
        .update(store.db().as_ref())
        .await
        .context("failed to update automation trigger event cursor")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn publish_project_id_event(store: &Store, project_id: i64) {
    match projects::project_name_by_id(store, project_id).await {
        Ok(project_name) => events::publish_automation_changed(&project_name),
        Err(err) => {
            tracing::warn!(
                project_id,
                error = %format_args!("{err:#}"),
                "failed to resolve project for automation trigger UI event"
            );
        }
    }
}

async fn new_item_created_events(
    store: &Store,
    project_id: i64,
    last_event_id: Option<i64>,
) -> Result<Vec<work_item_event::Model>> {
    let mut query = work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq("item_created"))
        .order_by_asc(work_item_event::Column::Id);
    if let Some(last_event_id) = last_event_id {
        query = query.filter(work_item_event::Column::Id.gt(last_event_id));
    }
    Ok(query
        .all(store.db().as_ref())
        .await
        .context("failed to load item-created events")?)
}

pub(crate) async fn latest_item_created_event_id(
    store: &Store,
    project_id: i64,
) -> Result<Option<i64>> {
    let event = work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq("item_created"))
        .order_by_desc(work_item_event::Column::Id)
        .limit(1)
        .one(store.db().as_ref())
        .await
        .context("failed to load latest item-created event")?;
    Ok(event.map(|event| event.id))
}

fn normalize_schedule(schedule: String) -> Result<String> {
    let schedule = schedule.trim();
    if schedule.is_empty() {
        bail!("automation schedule is required");
    }
    parse_schedule(schedule)?;
    Ok(schedule.to_owned())
}

pub(crate) fn validate_trigger_fields(
    name: &str,
    _activation: AutomationActivation,
    schedule: &str,
) -> Result<()> {
    if name.trim().is_empty() {
        bail!("automation trigger name cannot be empty");
    }
    parse_schedule(schedule)?;
    Ok(())
}

pub(crate) fn validate_trigger_configuration(
    name: &str,
    activation: AutomationActivation,
    effect: AutomationEffect,
    schedule: &str,
    work_item_selector: Option<&Condition>,
    prompt: &str,
) -> Result<()> {
    validate_trigger_fields(name, activation, schedule)?;
    if let Some(condition) = work_item_selector {
        label_conditions::validate_condition(condition)?;
    }
    if effect == AutomationEffect::ProduceWork {
        if matches!(
            activation,
            AutomationActivation::WorkItem | AutomationActivation::WorkItemCreated
        ) {
            bail!("work-producing automation must use manual or cron activation");
        }
        if prompt.trim().is_empty() {
            bail!("work-producing automation requires prompt text for the created item");
        }
        return Ok(());
    }
    if work_item_selector.is_none() {
        bail!("work-consuming automation requires a work item selector");
    }
    Ok(())
}

pub(crate) fn default_work_item_selector() -> Condition {
    default_automation_work_item_selector()
}

pub(crate) fn default_refinement_work_item_selector() -> Condition {
    needs_refinement_automation_work_item_selector()
}

pub(crate) fn default_verification_work_item_selector() -> Condition {
    needs_verification_automation_work_item_selector()
}

pub(crate) fn default_work_item_selector_storage() -> Result<String> {
    selector_to_storage(Some(&default_work_item_selector()))?
        .ok_or_else(|| report!("default work-item automation selector cannot be empty"))
}

async fn personality_id_for_effect(
    store: &Store,
    project_id: i64,
    effect: AutomationEffect,
    personality_id: Option<i64>,
) -> Result<Option<i64>> {
    if effect != AutomationEffect::ConsumeWork {
        return Ok(None);
    }
    let personality_id = match personality_id {
        Some(personality_id) => personality_id,
        None => personalities::default_personality_id(store, project_id).await?,
    };
    personalities::validate_personality_for_project(store, project_id, personality_id).await?;
    Ok(Some(personality_id))
}

fn default_project_automations() -> [DefaultProjectAutomation; 3] {
    [
        DefaultProjectAutomation {
            name: DEFAULT_WORK_ITEM_AUTOMATION_NAME,
            prompt: "",
            selector: default_work_item_selector,
            priority: 0,
            mutability: AutomationRunMutability::Mutating,
        },
        DefaultProjectAutomation {
            name: DEFAULT_REFINEMENT_AUTOMATION_NAME,
            prompt: DEFAULT_REFINEMENT_AUTOMATION_PROMPT,
            selector: default_refinement_work_item_selector,
            priority: REFINEMENT_AUTOMATION_PRIORITY,
            mutability: AutomationRunMutability::ReadOnly,
        },
        DefaultProjectAutomation {
            name: DEFAULT_VERIFICATION_AUTOMATION_NAME,
            prompt: DEFAULT_VERIFICATION_AUTOMATION_PROMPT,
            selector: default_verification_work_item_selector,
            priority: VERIFICATION_AUTOMATION_PRIORITY,
            mutability: AutomationRunMutability::ReadOnly,
        },
    ]
}

pub(crate) async fn ensure_default_project_automations_in_conn<C>(
    conn: &C,
    project_id: i64,
    default_tool: &str,
) -> Result<()>
where
    C: ConnectionTrait,
{
    for default in default_project_automations() {
        ensure_default_project_automation_in_conn(conn, project_id, default_tool, default).await?;
    }
    Ok(())
}

async fn ensure_default_project_automation_in_conn<C>(
    conn: &C,
    project_id: i64,
    default_tool: &str,
    default: DefaultProjectAutomation,
) -> Result<()>
where
    C: ConnectionTrait,
{
    let existing = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::Name.eq(default.name))
        .limit(1)
        .one(conn)
        .await
        .context("failed to check project automation defaults")?;
    if existing.is_some() {
        return Ok(());
    }

    let selector = (default.selector)();
    let personality_id = personalities::default_personality_id_in_conn(conn, project_id).await?;
    let now = utc_now();
    let trigger = AutomationTriggerActiveModel {
        project_id: Set(project_id),
        name: Set(default.name.to_owned()),
        enabled: Set(true),
        activation: Set(AutomationActivation::WorkItem.as_storage().to_owned()),
        effect: Set(AutomationEffect::ConsumeWork.as_storage().to_owned()),
        schedule: Set(DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE.to_owned()),
        tool_name: Set(default_tool.to_owned()),
        mutability: Set(default.mutability.as_storage().to_owned()),
        personality_id: Set(Some(personality_id)),
        prompt: Set(default.prompt.to_owned()),
        work_item_selector: Set(selector_to_storage(Some(&selector))?),
        priority: Set(default.priority),
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
    .insert(conn)
    .await
    .context("failed to create default project automation")?;

    automation_revisions::record_trigger_revision_in_conn(
        conn,
        &trigger,
        RevisionChangeOperation::Create,
        &RevisionActor::default(),
    )
    .await?;

    Ok(())
}

pub(crate) async fn ensure_default_project_automations(
    store: &Store,
    project_id: i64,
    default_tool: &str,
) -> Result<()> {
    ensure_default_project_automations_in_conn(store.db().as_ref(), project_id, default_tool).await
}

fn selector_for_activation(
    activation: AutomationActivation,
    selector: Option<Condition>,
) -> Result<Option<Condition>> {
    match (activation, selector) {
        (AutomationActivation::WorkItem, None) => Ok(Some(default_work_item_selector())),
        (_, selector) => Ok(selector),
    }
}

pub(crate) fn selector_to_storage(selector: Option<&Condition>) -> Result<Option<String>> {
    selector
        .map(|selector| -> Result<String> {
            label_conditions::validate_condition(selector)?;
            Ok(serde_json::to_string(selector).context("failed to encode work item selector")?)
        })
        .transpose()
}

pub(crate) fn selector_from_storage(selector: Option<&str>) -> Result<Option<Condition>> {
    selector
        .and_then(|selector| {
            let selector = selector.trim();
            (!selector.is_empty()).then_some(selector)
        })
        .map(|selector| {
            let condition = serde_json::from_str::<Condition>(selector)
                .context_with(|| format!("invalid work item selector JSON: {selector}"))?;
            label_conditions::validate_condition(&condition)?;
            Ok(condition)
        })
        .transpose()
}

fn trigger_is_due(next_evaluation_at: Option<&str>) -> bool {
    let Some(next_evaluation_at) = next_evaluation_at else {
        return true;
    };
    let Ok(next) = OffsetDateTime::parse(next_evaluation_at, &Rfc3339) else {
        return true;
    };
    next <= OffsetDateTime::now_utc()
}

pub(crate) fn next_evaluation_at(schedule: &str) -> Result<String> {
    let interval = parse_schedule(schedule)?;
    Ok((OffsetDateTime::now_utc() + interval)
        .format(&Rfc3339)
        .context("failed to format next trigger run time")?)
}

fn parse_schedule(schedule: &str) -> Result<Duration> {
    let value = schedule.trim();
    if value.eq_ignore_ascii_case("@hourly") {
        return Ok(Duration::hours(1));
    }
    if value.eq_ignore_ascii_case("@daily") {
        return Ok(Duration::days(1));
    }
    let value = value.strip_prefix("@every ").unwrap_or(value);
    let (number, suffix) = value.trim().split_at(
        value
            .trim()
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(value.trim().len()),
    );
    if number.is_empty() {
        bail!("schedule must be @hourly, @daily, @every <duration>, or seconds");
    }
    let amount: i64 = number
        .parse()
        .context_with(|| format!("invalid schedule amount '{number}'"))?;
    if amount < 1 {
        bail!("schedule interval must be at least 1");
    }
    match suffix.trim().to_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => Ok(Duration::seconds(amount)),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(Duration::minutes(amount)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Ok(Duration::hours(amount)),
        "d" | "day" | "days" => Ok(Duration::days(amount)),
        other => bail!("unsupported schedule suffix '{other}'"),
    }
}

pub(crate) fn model_to_view(trigger: AutomationTriggerModel) -> Result<AutomationTriggerView> {
    let produced_work = trigger
        .produced_work_spec_json
        .as_deref()
        .map(|value| {
            serde_json::from_str::<ProducedWorkSpec>(value)
                .context("invalid produced-work specification")
        })
        .transpose()?;
    let postconditions = trigger
        .postconditions_json
        .as_deref()
        .map(|value| {
            serde_json::from_str::<AutomationPostconditions>(value)
                .context("invalid automation postconditions")
        })
        .transpose()?;
    let reasoning_effort = trigger
        .reasoning_effort_override
        .as_deref()
        .map(str::parse::<AgentReasoningEffort>)
        .transpose()
        .context("invalid automation reasoning-effort override")?;
    let timeout_seconds = trigger
        .timeout_seconds
        .map(|value| u64::try_from(value).context("automation timeout must be positive"))
        .transpose()?;
    let max_concurrent_runs = trigger
        .max_concurrent_runs
        .map(|value| u64::try_from(value).context("automation run limit must be positive"))
        .transpose()?;
    Ok(AutomationTriggerView {
        id: trigger.id,
        project_id: trigger.project_id,
        name: trigger.name,
        enabled: trigger.enabled,
        activation: AutomationActivation::from_str(&trigger.activation)?,
        effect: AutomationEffect::from_str(&trigger.effect)?,
        schedule: trigger.schedule,
        tool_name: AgentToolName::from_str(&trigger.tool_name)?,
        mutability: AutomationRunMutability::from_str(&trigger.mutability)?,
        personality_id: trigger.personality_id,
        personality_name: None,
        prompt: trigger.prompt,
        work_item_selector: selector_from_storage(trigger.work_item_selector.as_deref())?,
        priority: trigger.priority,
        exclusive: trigger.exclusive,
        produced_work,
        execution: AutomationExecutionPolicy {
            model: projects::normalize_optional(trigger.model_override),
            reasoning_effort,
            timeout_seconds,
            max_concurrent_runs,
            concurrency_group: projects::normalize_optional(trigger.concurrency_group),
        },
        postconditions,
        current_revision_id: trigger.current_revision_id,
        managed_bundle_key: trigger.managed_bundle_key,
        managed_object_key: trigger.managed_object_key,
        evaluation_count: trigger.evaluation_count,
        pending_evaluation_count: trigger.pending_evaluation_count,
        last_evaluation_queued_at: trigger.last_evaluation_queued_at,
        last_evaluated_at: trigger.last_evaluated_at,
        next_evaluation_at: trigger.next_evaluation_at,
        last_event_id: trigger.last_event_id,
        created_at: trigger.created_at,
        updated_at: trigger.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::Condition;
    use assertr::prelude::*;
    use std::path::PathBuf;

    use crudkit_core::condition::{
        ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        agent_tools::set_tool_path,
        automation_revisions, automation_routing, item_claims,
        item_label_service::add_label,
        items::{CreateWorkItem, create_item, get_item},
        label_conditions, personalities,
        projects::{CreateProject, create_project},
    };
    use crate::shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, CreateWorkItemLabelRequest, FEEDBACK_REQUESTED_LABEL_KEY,
        RoutingExplainRequest, STATE_LABEL_KEY, WorkItemView,
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

        let evaluations =
            automation_revisions::list_evaluations(&store, "demo", Some(trigger.id), 20)
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
        assert_that!(&(items::list_items(&store, "demo", None).await.unwrap().len()))
            .is_equal_to(1);
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
        assert_that!(&(automation.work_item_selector))
            .is_equal_to(Some(default_work_item_selector()));
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

        assert_that!(&(!item_matches_selector(&refine_item, &default_work_item_selector())))
            .is_true();
        assert_that!(
            &(item_matches_selector(&refine_item, &default_refinement_work_item_selector()))
        )
        .is_true();
        assert_that!(&(!item_matches_selector(&verify_item, &default_work_item_selector())))
            .is_true();
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
        assert_that!(&(item.labels
                .iter()
                .all(|label| label.key != crate::shared::view_models::AUTOMATION_BLOCKED_LABEL_KEY)))
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

        let active_projects = vec!["demo".to_owned()];
        let outcomes = run_due_triggers_with_sessions_for_projects(
            &store,
            None,
            None,
            Some(&active_projects),
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
}
