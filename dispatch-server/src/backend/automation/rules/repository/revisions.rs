use super::super::model::{PersonalityReference, RuleFields};
use super::encoding::{decode_trigger_policy, selector_from_storage};
use crate::backend::{
    automation::revisions::model::RevisionActor,
    entities::{
        automation_trigger::{AutomationTriggerActiveModel, AutomationTriggerModel},
        automation_trigger_revision::{
            self, AutomationTriggerRevision, AutomationTriggerRevisionActiveModel,
        },
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::{AutomationRevisionView, RevisionChangeOperation};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
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
pub(crate) async fn record_in_conn<C>(
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
pub(crate) async fn list_in(
    transaction: &Transaction,
    project_id: i64,
    trigger_id: i64,
) -> Result<Vec<AutomationRevisionView>> {
    AutomationTriggerRevision::find()
        .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
        .filter(automation_trigger_revision::Column::TriggerId.eq(trigger_id))
        .order_by_desc(automation_trigger_revision::Column::RevisionNumber)
        .all(transaction.connection())
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
pub(crate) async fn fields_in(
    transaction: &Transaction,
    project_id: i64,
    id: i64,
    revision_id: i64,
) -> Result<RuleFields> {
    let revision = AutomationTriggerRevision::find_by_id(revision_id)
        .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
        .filter(automation_trigger_revision::Column::TriggerId.eq(id))
        .one(transaction.connection())
        .await
        .context("failed to load automation revision")?
        .ok_or_else(|| report!("revision {revision_id} does not belong to trigger {id}"))?;
    let config: TriggerRevisionConfiguration = serde_json::from_str(&revision.configuration_json)
        .context("invalid stored automation revision")?;
    let effect = config.effect.parse()?;
    let policy = decode_trigger_policy(
        effect,
        config.produced_work_spec_json.as_deref(),
        config.postconditions_json.as_deref(),
        config.model_override.as_deref(),
        config.reasoning_effort_override.as_deref(),
        config.timeout_seconds,
        config.max_concurrent_runs,
        config.concurrency_group.as_deref(),
    )?;
    Ok(RuleFields {
        name: config.name,
        enabled: config.enabled,
        activation: config.activation.parse()?,
        effect,
        schedule: config.schedule,
        tool_name: Some(config.tool_name.parse()?),
        mutability: config.mutability.parse()?,
        personality: config.personality_id.map(PersonalityReference::Id),
        prompt: config.prompt,
        selector: selector_from_storage(config.work_item_selector.as_deref())?,
        priority: config.priority,
        exclusive: config.exclusive,
        produced_work: policy.produced_work,
        execution: policy.execution,
        postconditions: policy.postconditions,
    })
}
