use rootcause::{Result, prelude::*};

use super::configuration::validate_trigger_configuration;
use crate::{
    backend::{
        automation_admission, automation_postconditions, item_labels, projects, workflow_labels,
    },
    shared::view_models::{
        AutomationEffect, AutomationExecutionPolicy, AutomationPostconditions, AutomationRuleInput,
        ProduceDeduplication, ProducedWorkSpec,
    },
};

/// Typed policy decoded from persistence before a rule can be used by a service.
pub(crate) struct TriggerPolicy {
    pub(crate) produced_work: Option<ProducedWorkSpec>,
    pub(crate) execution: AutomationExecutionPolicy,
    pub(crate) postconditions: Option<AutomationPostconditions>,
}

pub(crate) fn validate_rule_input(input: &AutomationRuleInput) -> Result<()> {
    validate_trigger_configuration(
        &input.name,
        input.activation,
        input.effect,
        &input.schedule,
        input.selector.as_ref(),
        &input.prompt_markdown,
    )?;
    validate_rule_policy(
        input.effect,
        input.produced_work.as_ref(),
        &input.execution,
        input.postconditions.as_ref(),
    )
}

fn validate_rule_policy(
    effect: AutomationEffect,
    produced_work: Option<&ProducedWorkSpec>,
    execution: &AutomationExecutionPolicy,
    postconditions: Option<&AutomationPostconditions>,
) -> Result<()> {
    match effect {
        AutomationEffect::ConsumeWork if produced_work.is_some() => {
            bail!("produced-work specification is only valid for produce_work automations");
        }
        AutomationEffect::ProduceWork if postconditions.is_some() => {
            bail!("postconditions are only valid for consume_work automations");
        }
        _ => {}
    }
    if let Some(spec) = produced_work {
        validate_produced_work_spec(spec)?;
    }
    if let Some(postconditions) = postconditions {
        automation_postconditions::validate_configuration(postconditions, "postconditions")?;
    }
    automation_admission::validate_execution_policy(execution)?;
    projects::validate_agent_model_field("automation model override", execution.model.as_deref())?;
    projects::validate_agent_model_reasoning_effort(
        "automation model override",
        execution.model.as_deref(),
        "automation reasoning-effort override",
        execution.reasoning_effort,
    )
}

/// The storage adapter is shared by CrudKit writes and persisted-rule reads.
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

pub(super) fn validate_produced_work_spec(spec: &ProducedWorkSpec) -> Result<()> {
    if let Some(title) = &spec.title
        && title.trim().is_empty()
    {
        bail!("produced-work title cannot be empty when configured");
    }
    if let ProduceDeduplication::WhileUnfinishedForKey { key } = &spec.deduplication {
        validate_stable_key("produced-work deduplication key", key)?;
    }
    workflow_labels::normalize_state_value(spec.state.clone())?;
    item_labels::normalize_initial_labels(
        spec.initial_labels
            .iter()
            .map(|label| (label.key.clone(), label.value.clone())),
    )
    .context("invalid produced-work initial labels")?;
    let model = projects::normalize_optional(spec.agent_model_override.clone());
    projects::validate_agent_model_field("produced-work model override", model.as_deref())?;
    projects::validate_agent_model_reasoning_effort(
        "produced-work model override",
        model.as_deref(),
        "produced-work reasoning-effort override",
        spec.agent_reasoning_effort_override,
    )
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
