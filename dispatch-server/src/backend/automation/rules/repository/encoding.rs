use super::super::policy::{TriggerPolicy, validate_rule_policy};
use crate::backend::{
    entities::automation_trigger::{AutomationTriggerModel, Model},
    items::labels::conditions as label_conditions,
};
use crudkit_core::condition::Condition;
use dispatch_types::*;
use rootcause::{Result, prelude::*};
use std::str::FromStr;
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_trigger_policy(
    effect: AutomationEffect,
    produced_work_spec_json: Option<&str>,
    postconditions_json: Option<&str>,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    timeout_seconds: Option<i64>,
    max_concurrent_runs: Option<i64>,
    concurrency_group: Option<&str>,
) -> Result<TriggerPolicy> {
    let policy = TriggerPolicy {
        produced_work: produced_work_spec_json
            .map(serde_json::from_str)
            .transpose()
            .context("invalid produced-work specification JSON")?,
        postconditions: postconditions_json
            .map(serde_json::from_str)
            .transpose()
            .context("invalid semantic postconditions JSON")?,
        execution: AutomationExecutionPolicy {
            model: model.map(str::to_owned),
            reasoning_effort: reasoning_effort
                .map(str::parse)
                .transpose()
                .context("invalid automation reasoning-effort override")?,
            timeout_seconds: timeout_seconds
                .map(|value| u64::try_from(value).context("automation timeout must be positive"))
                .transpose()?,
            max_concurrent_runs: max_concurrent_runs
                .map(|value| {
                    u64::try_from(value).context("automation concurrent-run limit must be positive")
                })
                .transpose()?,
            concurrency_group: concurrency_group.map(str::to_owned),
        },
    };
    validate_rule_policy(
        effect,
        policy.produced_work.as_ref(),
        &policy.execution,
        policy.postconditions.as_ref(),
    )?;
    Ok(policy)
}
pub(crate) fn default_work_item_selector_storage() -> Result<String> {
    selector_to_storage(Some(
        &super::super::configuration::default_work_item_selector(),
    ))?
    .ok_or_else(|| report!("default work-item automation selector cannot be empty"))
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
pub(crate) fn model_to_view(trigger: AutomationTriggerModel) -> Result<AutomationTriggerView> {
    let effect = AutomationEffect::from_str(&trigger.effect)?;
    let policy = decode_trigger_policy(
        effect,
        trigger.produced_work_spec_json.as_deref(),
        trigger.postconditions_json.as_deref(),
        trigger.model_override.as_deref(),
        trigger.reasoning_effort_override.as_deref(),
        trigger.timeout_seconds,
        trigger.max_concurrent_runs,
        trigger.concurrency_group.as_deref(),
    )
    .context_with(|| {
        format!(
            "invalid policy for automation trigger {} ('{}')",
            trigger.id, trigger.name
        )
    })?;
    Ok(AutomationTriggerView {
        id: trigger.id,
        project_id: trigger.project_id,
        name: trigger.name,
        enabled: trigger.enabled,
        activation: AutomationActivation::from_str(&trigger.activation)?,
        effect,
        schedule: trigger.schedule,
        tool_name: AgentToolName::from_str(&trigger.tool_name)?,
        mutability: AutomationRunMutability::from_str(&trigger.mutability)?,
        personality_id: trigger.personality_id,
        personality_name: None,
        prompt: trigger.prompt,
        work_item_selector: selector_from_storage(trigger.work_item_selector.as_deref())?,
        priority: trigger.priority,
        exclusive: trigger.exclusive,
        produced_work: policy.produced_work,
        execution: policy.execution,
        postconditions: policy.postconditions,
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
pub(crate) fn encode(record: AutomationTriggerView) -> Result<AutomationTriggerModel> {
    Ok(Model {
        id: record.id,
        project_id: record.project_id,
        name: record.name,
        enabled: record.enabled,
        activation: record.activation.as_storage().to_owned(),
        effect: record.effect.as_storage().to_owned(),
        schedule: record.schedule,
        tool_name: record.tool_name.as_storage().to_owned(),
        mutability: record.mutability.as_storage().to_owned(),
        personality_id: record.personality_id,
        prompt: record.prompt,
        work_item_selector: selector_to_storage(record.work_item_selector.as_ref())?,
        priority: record.priority,
        exclusive: record.exclusive,
        produced_work_spec_json: record
            .produced_work
            .map(|value| serde_json::to_string(&value))
            .transpose()?,
        postconditions_json: record
            .postconditions
            .map(|value| serde_json::to_string(&value))
            .transpose()?,
        model_override: record.execution.model,
        reasoning_effort_override: record
            .execution
            .reasoning_effort
            .map(|value| value.as_storage().to_owned()),
        timeout_seconds: record
            .execution
            .timeout_seconds
            .map(i64::try_from)
            .transpose()?,
        max_concurrent_runs: record
            .execution
            .max_concurrent_runs
            .map(i64::try_from)
            .transpose()?,
        concurrency_group: record.execution.concurrency_group,
        current_revision_id: record.current_revision_id,
        managed_bundle_key: record.managed_bundle_key,
        managed_object_key: record.managed_object_key,
        evaluation_count: record.evaluation_count,
        pending_evaluation_count: record.pending_evaluation_count,
        last_evaluation_queued_at: record.last_evaluation_queued_at,
        last_evaluated_at: record.last_evaluated_at,
        next_evaluation_at: record.next_evaluation_at,
        last_event_id: record.last_event_id,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}
