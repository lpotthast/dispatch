use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    backend::{
        entities::{
            agent_run::{self, AgentRun},
            automation_trigger::{
                AutomationTrigger, AutomationTriggerActiveModel, AutomationTriggerModel,
            },
            automation_trigger_revision::{
                self, AutomationTriggerRevision, AutomationTriggerRevisionActiveModel,
            },
            personality::{Personality, PersonalityActiveModel, PersonalityModel},
            personality_revision::{self, PersonalityRevision, PersonalityRevisionActiveModel},
            work_item_origin::{self, WorkItemOrigin},
        },
        projects,
        storage::{Store, utc_now},
    },
    shared::view_models::{
        AgentRunStatus, AuthorType, AutomationActivation, AutomationEvaluationView,
        AutomationRevisionView, PersonalityRevisionView, RevisionAnalyticsView,
        RevisionChangeOperation,
    },
};

use crate::backend::entities::automation_evaluation::{self, AutomationEvaluation};

#[derive(Deserialize)]
struct TriggerRevisionConfiguration {
    activation: String,
    concurrency_group: Option<String>,
    effect: String,
    enabled: bool,
    exclusive: bool,
    max_concurrent_runs: Option<i64>,
    model_override: Option<String>,
    mutability: String,
    name: String,
    personality_id: Option<i64>,
    postconditions_json: Option<String>,
    priority: i64,
    produced_work_spec_json: Option<String>,
    prompt: String,
    reasoning_effort_override: Option<String>,
    schedule: String,
    timeout_seconds: Option<i64>,
    tool_name: String,
    work_item_selector: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RevisionActor {
    pub(crate) actor_type: Option<AuthorType>,
    pub(crate) actor_id: Option<String>,
}

pub(crate) fn canonical_trigger_configuration(
    trigger: &AutomationTriggerModel,
) -> Result<(String, String)> {
    let value = serde_json::json!({
        "activation": trigger.activation,
        "concurrency_group": trigger.concurrency_group,
        "effect": trigger.effect,
        "enabled": trigger.enabled,
        "exclusive": trigger.exclusive,
        "managed_bundle_key": trigger.managed_bundle_key,
        "managed_object_key": trigger.managed_object_key,
        "max_concurrent_runs": trigger.max_concurrent_runs,
        "model_override": trigger.model_override,
        "mutability": trigger.mutability,
        "name": trigger.name,
        "personality_id": trigger.personality_id,
        "postconditions_json": trigger.postconditions_json,
        "priority": trigger.priority,
        "produced_work_spec_json": trigger.produced_work_spec_json,
        "prompt": trigger.prompt,
        "reasoning_effort_override": trigger.reasoning_effort_override,
        "schedule": trigger.schedule,
        "timeout_seconds": trigger.timeout_seconds,
        "tool_name": trigger.tool_name,
        "work_item_selector": trigger.work_item_selector,
    });
    let canonical =
        serde_json::to_string(&value).context("failed to encode automation revision")?;
    let sha256 = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    Ok((canonical, sha256))
}

pub(crate) async fn record_trigger_revision_in_conn<C>(
    conn: &C,
    trigger: &AutomationTriggerModel,
    operation: RevisionChangeOperation,
    actor: &RevisionActor,
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let revision_number = AutomationTriggerRevision::find()
        .filter(automation_trigger_revision::Column::TriggerId.eq(trigger.id))
        .order_by_desc(automation_trigger_revision::Column::RevisionNumber)
        .one(conn)
        .await
        .context("failed to load current automation revision")?
        .map(|revision| revision.revision_number.saturating_add(1))
        .unwrap_or(1);
    let (configuration_json, sha256) = canonical_trigger_configuration(trigger)?;
    let revision = AutomationTriggerRevisionActiveModel {
        trigger_id: Set(Some(trigger.id)),
        project_id: Set(trigger.project_id),
        trigger_name: Set(trigger.name.clone()),
        revision_number: Set(revision_number),
        configuration_json: Set(configuration_json),
        sha256: Set(sha256),
        change_operation: Set(operation.as_storage().to_owned()),
        actor_type: Set(actor.actor_type.map(|kind| kind.as_storage().to_owned())),
        actor_id: Set(actor.actor_id.clone()),
        created_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to create automation revision")?;

    let mut active: AutomationTriggerActiveModel = trigger.clone().into();
    active.current_revision_id = Set(Some(revision.id));
    active
        .update(conn)
        .await
        .context("failed to set current automation revision")?;
    Ok(revision.id)
}

pub(crate) async fn record_personality_revision_in_conn<C>(
    conn: &C,
    personality: &PersonalityModel,
    operation: RevisionChangeOperation,
    actor: &RevisionActor,
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let revision_number = PersonalityRevision::find()
        .filter(personality_revision::Column::PersonalityId.eq(personality.id))
        .order_by_desc(personality_revision::Column::RevisionNumber)
        .one(conn)
        .await
        .context("failed to load current personality revision")?
        .map(|revision| revision.revision_number.saturating_add(1))
        .unwrap_or(1);
    let canonical = serde_json::to_string(&(
        personality.name.as_str(),
        personality.personality_description.as_str(),
        personality.managed_bundle_key.as_deref(),
        personality.managed_object_key.as_deref(),
    ))
    .context("failed to encode personality revision")?;
    let sha256 = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    let revision = PersonalityRevisionActiveModel {
        personality_id: Set(Some(personality.id)),
        project_id: Set(personality.project_id),
        personality_name: Set(personality.name.clone()),
        revision_number: Set(revision_number),
        personality_description: Set(personality.personality_description.clone()),
        sha256: Set(sha256),
        change_operation: Set(operation.as_storage().to_owned()),
        actor_type: Set(actor.actor_type.map(|kind| kind.as_storage().to_owned())),
        actor_id: Set(actor.actor_id.clone()),
        created_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to create personality revision")?;
    let mut active: PersonalityActiveModel = personality.clone().into();
    active.current_revision_id = Set(Some(revision.id));
    active
        .update(conn)
        .await
        .context("failed to set current personality revision")?;
    Ok(revision.id)
}

pub(crate) async fn list_trigger_revisions(
    store: &Store,
    project_id: i64,
    trigger_id: i64,
) -> Result<Vec<AutomationRevisionView>> {
    AutomationTriggerRevision::find()
        .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
        .filter(automation_trigger_revision::Column::TriggerId.eq(trigger_id))
        .order_by_desc(automation_trigger_revision::Column::RevisionNumber)
        .all(store.db().as_ref())
        .await
        .context("failed to list automation revisions")?
        .into_iter()
        .map(|revision| {
            Ok(AutomationRevisionView {
                id: revision.id,
                trigger_id: revision.trigger_id,
                project_id: revision.project_id,
                revision_number: revision.revision_number,
                configuration: serde_json::from_str(&revision.configuration_json)
                    .context("invalid stored automation revision")?,
                sha256: revision.sha256,
                operation: revision.change_operation.parse()?,
                actor_type: revision.actor_type.as_deref().map(str::parse).transpose()?,
                actor_id: revision.actor_id,
                created_at: revision.created_at,
            })
        })
        .collect()
}

pub(crate) async fn list_personality_revisions(
    store: &Store,
    project_id: i64,
    personality_id: i64,
) -> Result<Vec<PersonalityRevisionView>> {
    PersonalityRevision::find()
        .filter(personality_revision::Column::ProjectId.eq(project_id))
        .filter(personality_revision::Column::PersonalityId.eq(personality_id))
        .order_by_desc(personality_revision::Column::RevisionNumber)
        .all(store.db().as_ref())
        .await
        .context("failed to list personality revisions")?
        .into_iter()
        .map(|revision| {
            Ok(PersonalityRevisionView {
                id: revision.id,
                personality_id: revision.personality_id,
                project_id: revision.project_id,
                revision_number: revision.revision_number,
                name: revision.personality_name,
                personality_description: revision.personality_description,
                sha256: revision.sha256,
                operation: revision.change_operation.parse()?,
                actor_type: revision.actor_type.as_deref().map(str::parse).transpose()?,
                actor_id: revision.actor_id,
                created_at: revision.created_at,
            })
        })
        .collect()
}

pub(crate) async fn restore_trigger_revision(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
    revision_id: i64,
) -> Result<AutomationTriggerModel> {
    let project_id = projects::project_id(store, project_name).await?;
    let current = AutomationTrigger::find_by_id(trigger_id)
        .filter(crate::backend::entities::automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;
    if current.managed_bundle_key.is_some() {
        bail!("bundle-managed automations must be detached before restoring a revision");
    }
    let revision = AutomationTriggerRevision::find_by_id(revision_id)
        .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
        .filter(automation_trigger_revision::Column::TriggerId.eq(trigger_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation revision")?
        .ok_or_else(|| report!("revision {revision_id} does not belong to trigger {trigger_id}"))?;
    let config: TriggerRevisionConfiguration = serde_json::from_str(&revision.configuration_json)
        .context("invalid stored automation revision")?;
    config.activation.parse::<AutomationActivation>()?;
    let mut active: AutomationTriggerActiveModel = current.into();
    active.name = Set(config.name);
    active.enabled = Set(config.enabled);
    active.activation = Set(config.activation);
    active.effect = Set(config.effect);
    active.schedule = Set(config.schedule);
    active.tool_name = Set(config.tool_name);
    active.mutability = Set(config.mutability);
    active.personality_id = Set(config.personality_id);
    active.prompt = Set(config.prompt);
    active.work_item_selector = Set(config.work_item_selector);
    active.priority = Set(config.priority);
    active.exclusive = Set(config.exclusive);
    active.produced_work_spec_json = Set(config.produced_work_spec_json);
    active.postconditions_json = Set(config.postconditions_json);
    active.model_override = Set(config.model_override);
    active.reasoning_effort_override = Set(config.reasoning_effort_override);
    active.timeout_seconds = Set(config.timeout_seconds);
    active.max_concurrent_runs = Set(config.max_concurrent_runs);
    active.concurrency_group = Set(config.concurrency_group);
    active.updated_at = Set(utc_now());
    let trigger = active
        .update(store.db().as_ref())
        .await
        .context("failed to restore automation revision")?;
    record_trigger_revision_in_conn(
        store.db().as_ref(),
        &trigger,
        RevisionChangeOperation::Restore,
        &RevisionActor::default(),
    )
    .await?;
    AutomationTrigger::find_by_id(trigger_id)
        .one(store.db().as_ref())
        .await
        .context("failed to reload restored automation")?
        .ok_or_else(|| report!("restored automation disappeared"))
}

pub(crate) async fn restore_personality_revision(
    store: &Store,
    project_name: &str,
    personality_id: i64,
    revision_id: i64,
) -> Result<PersonalityModel> {
    let project_id = projects::project_id(store, project_name).await?;
    let current = Personality::find_by_id(personality_id)
        .filter(crate::backend::entities::personality::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load personality")?
        .ok_or_else(|| report!("personality {personality_id} does not exist in this project"))?;
    if current.managed_bundle_key.is_some() {
        bail!("bundle-managed personalities must be detached before restoring a revision");
    }
    let revision = PersonalityRevision::find_by_id(revision_id)
        .filter(personality_revision::Column::ProjectId.eq(project_id))
        .filter(personality_revision::Column::PersonalityId.eq(personality_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load personality revision")?
        .ok_or_else(|| {
            report!("revision {revision_id} does not belong to personality {personality_id}")
        })?;
    let mut active: PersonalityActiveModel = current.into();
    active.name = Set(revision.personality_name);
    active.personality_description = Set(revision.personality_description);
    active.updated_at = Set(utc_now());
    let personality = active
        .update(store.db().as_ref())
        .await
        .context("failed to restore personality revision")?;
    record_personality_revision_in_conn(
        store.db().as_ref(),
        &personality,
        RevisionChangeOperation::Restore,
        &RevisionActor::default(),
    )
    .await?;
    Personality::find_by_id(personality_id)
        .one(store.db().as_ref())
        .await
        .context("failed to reload restored personality")?
        .ok_or_else(|| report!("restored personality disappeared"))
}

pub(crate) async fn list_evaluations(
    store: &Store,
    project_name: &str,
    trigger_id: Option<i64>,
    limit: u64,
) -> Result<Vec<AutomationEvaluationView>> {
    let project_id = projects::project_id(store, project_name).await?;
    let mut query = AutomationEvaluation::find()
        .filter(automation_evaluation::Column::ProjectId.eq(project_id));
    if let Some(trigger_id) = trigger_id {
        query = query.filter(automation_evaluation::Column::TriggerId.eq(trigger_id));
    }
    query
        .order_by_desc(automation_evaluation::Column::Id)
        .limit(limit.clamp(1, 500))
        .all(store.db().as_ref())
        .await
        .context("failed to list automation evaluations")?
        .into_iter()
        .map(|evaluation| {
            Ok(AutomationEvaluationView {
                id: evaluation.id,
                project_id: evaluation.project_id,
                trigger_id: evaluation.trigger_id,
                trigger_revision_id: evaluation.trigger_revision_id,
                trigger_name: evaluation.trigger_name,
                activation_cause: evaluation.activation_cause,
                outcome: evaluation.outcome.parse()?,
                work_item_id: evaluation.work_item_id,
                run_id: evaluation.run_id,
                error: evaluation.error,
                created_at: evaluation.created_at,
                completed_at: evaluation.completed_at,
            })
        })
        .collect()
}

pub(crate) async fn trigger_revision_analytics(
    store: &Store,
    project_name: &str,
    revision_id: i64,
) -> Result<RevisionAnalyticsView> {
    let project_id = projects::project_id(store, project_name).await?;
    let revision = AutomationTriggerRevision::find_by_id(revision_id)
        .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation revision")?
        .ok_or_else(|| report!("automation revision {revision_id} does not exist"))?;
    let runs = AgentRun::find()
        .filter(agent_run::Column::ProjectId.eq(project_id))
        .filter(agent_run::Column::TriggerRevisionId.eq(revision.id))
        .all(store.db().as_ref())
        .await
        .context("failed to load revision runs")?;
    let run_ids = runs.iter().map(|run| run.id).collect::<Vec<_>>();
    let origins = WorkItemOrigin::find()
        .filter(work_item_origin::Column::ProjectId.eq(project_id))
        .all(store.db().as_ref())
        .await
        .context("failed to load revision item origins")?;
    let mut analytics = RevisionAnalyticsView {
        revision_id,
        run_count: runs.len() as u64,
        created_item_count: origins
            .iter()
            .filter(|origin| {
                origin.trigger_revision_id == Some(revision_id)
                    || origin.agent_run_id.is_some_and(|id| run_ids.contains(&id))
            })
            .count() as u64,
        ..Default::default()
    };
    for run in runs {
        match run.status.parse::<AgentRunStatus>()? {
            AgentRunStatus::Completed => analytics.completed_count += 1,
            AgentRunStatus::Failed => analytics.failed_count += 1,
            _ => {}
        }
        match run.semantic_postcondition_status.as_str() {
            "passed" => analytics.semantic_passed_count += 1,
            "failed" => analytics.semantic_failed_count += 1,
            _ => {}
        }
        analytics.input_tokens += run.input_tokens.unwrap_or_default().max(0) as u64;
        analytics.cached_input_tokens += run.cached_input_tokens.unwrap_or_default().max(0) as u64;
        analytics.output_tokens += run.output_tokens.unwrap_or_default().max(0) as u64;
        if let (Some(started), Some(finished)) = (run.started_at, run.finished_at)
            && let (Ok(started), Ok(finished)) = (
                OffsetDateTime::parse(&started, &Rfc3339),
                OffsetDateTime::parse(&finished, &Rfc3339),
            )
        {
            analytics.total_duration_seconds += (finished - started).whole_seconds().max(0) as u64;
        }
    }
    Ok(analytics)
}
