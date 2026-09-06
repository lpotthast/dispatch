use std::str::FromStr;

use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::{
    backend::{
        automation_revisions::{self, RevisionActor},
        entities::automation_trigger::{self, AutomationTrigger, AutomationTriggerActiveModel},
        events, personalities, projects,
        storage::{Store, utc_now},
    },
    shared::view_models::{
        AgentToolName, AutomationActivation, AutomationEffect, AutomationRuleInput,
        AutomationRunMutability, AutomationTriggerView, RevisionChangeOperation,
    },
};

mod configuration;
mod execution;
mod policy;

pub(crate) use configuration::{
    default_work_item_selector_storage, ensure_default_project_automations,
    ensure_default_project_automations_in_conn, model_to_view, next_evaluation_at,
    selector_from_storage, selector_to_storage, validate_trigger_configuration,
};
use configuration::{normalize_schedule, personality_id_for_effect, selector_for_activation};
pub(crate) use execution::latest_item_created_event_id;
pub use execution::{schedule_trigger_evaluation, spawn_scheduler_until};
pub(crate) use policy::{decode_trigger_policy, validate_rule_input, validate_stable_key};

#[cfg(test)]
use configuration::{
    DEFAULT_REFINEMENT_AUTOMATION_NAME, DEFAULT_VERIFICATION_AUTOMATION_NAME,
    DEFAULT_WORK_ITEM_AUTOMATION_NAME, DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE,
    REFINEMENT_AUTOMATION_PRIORITY, VERIFICATION_AUTOMATION_PRIORITY,
    default_refinement_work_item_selector, default_verification_work_item_selector,
    default_work_item_selector, parse_schedule,
};
#[cfg(test)]
use execution::{
    PRIORITY_SCORE_SECONDS, WorkItemAutomationCandidate, create_work_item_from_trigger,
    run_due_triggers, run_due_triggers_with_sessions_for_projects,
    run_first_available_work_item_automation_candidate, run_next_work_item_automation_for_project,
    work_item_automation_score,
};
#[cfg(test)]
mod tests;

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
    input.selector = selector_for_activation(input.activation, input.selector.take())?;
    validate_rule_input(&input)?;
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
        work_item_selector: Set(selector_to_storage(input.selector.as_ref())?),
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
    input.selector = selector_for_activation(input.activation, input.selector.take())?;
    validate_rule_input(&input)?;
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
    active.work_item_selector = Set(selector_to_storage(input.selector.as_ref())?);
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
    decode_trigger_policy(
        update.effect,
        existing.produced_work_spec_json.as_deref(),
        existing.postconditions_json.as_deref(),
        existing.model_override.as_deref(),
        existing.reasoning_effort_override.as_deref(),
        existing.timeout_seconds,
        existing.max_concurrent_runs,
        existing.concurrency_group.as_deref(),
    )?;
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
