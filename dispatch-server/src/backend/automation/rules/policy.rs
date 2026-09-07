use rootcause::{Result, prelude::*};

use super::configuration::validate_trigger_configuration;
use crate::{
    backend::{
        automation::postconditions::policy as automation_postconditions,
        items::labels::policy as item_labels, items::labels::workflow as workflow_labels, projects,
    },
    shared::view_models::{
        AutomationEffect, AutomationExecutionPolicy, AutomationPostconditions, AutomationRuleInput,
        ProduceDeduplication, ProducedWorkSpec,
    },
};

/// Typed policy decoded from persistence before a rule can be used by a service.
#[derive(Clone, Debug)]
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

pub(crate) fn validate_rule_policy(
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
    crate::backend::runs::admission::policy::validate_execution_policy(execution)?;
    projects::validate_agent_model_field("automation model override", execution.model.as_deref())?;
    projects::validate_agent_model_reasoning_effort(
        "automation model override",
        execution.model.as_deref(),
        "automation reasoning-effort override",
        execution.reasoning_effort,
    )
}

pub(crate) fn validate_produced_work_spec(spec: &ProducedWorkSpec) -> Result<()> {
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
