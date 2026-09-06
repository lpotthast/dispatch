use std::str::FromStr;

use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    backend::{
        automation_revisions::{self, RevisionActor},
        entities::automation_trigger::{
            self, AutomationTrigger, AutomationTriggerActiveModel, AutomationTriggerModel,
        },
        label_conditions, personalities,
        storage::{Store, utc_now},
    },
    shared::view_models::{
        AgentToolName, AutomationActivation, AutomationEffect, AutomationRunMutability,
        AutomationTriggerView, RevisionChangeOperation, default_automation_work_item_selector,
        needs_refinement_automation_work_item_selector,
        needs_verification_automation_work_item_selector,
    },
};

pub(super) const DEFAULT_WORK_ITEM_AUTOMATION_NAME: &str = "Claim open work";
pub(super) const DEFAULT_REFINEMENT_AUTOMATION_NAME: &str = "Refine needs-refinement work";
pub(super) const DEFAULT_VERIFICATION_AUTOMATION_NAME: &str = "Verify needs-verification work";
pub(super) const DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE: &str = "@every 15s";
pub(super) const REFINEMENT_AUTOMATION_PRIORITY: i64 = 20;
pub(super) const VERIFICATION_AUTOMATION_PRIORITY: i64 = 10;

const DEFAULT_REFINEMENT_AUTOMATION_PROMPT: &str = r#"You are the needs-refinement executor for the claimed Dispatch work item.

Goal: turn a rough or under-specified item into implementation-ready work. Do not implement the work.

Required workflow:
- Re-read the item, comments, and labels, then complete the required root-first Dispatch knowledge navigation before editing it.
- Clarify the title and description so a later implementation agent can act without guessing. Prefer concrete scope, non-goals, acceptance criteria, suggested approach, verification expectations, and open questions only when human input is genuinely required.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-refinement` label when refinement is complete. Keep or add `needs-verification` only when the refined item should be checked before implementation.
- Add a concise progress comment summarizing what changed.

Do not call `dispatch item finish` for successful refinement, and do not call `dispatch item release` after successful refinement. Let Dispatch release the temporary claim after your final response. If the item cannot be refined without a human decision, leave `needs-refinement` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user."#;

const DEFAULT_VERIFICATION_AUTOMATION_PROMPT: &str = r#"You are the needs-verification executor for the claimed Dispatch work item.

Goal: verify whether the item is necessary, accurate, and ready for a later implementation agent. Do not implement the work.

Required workflow:
- Re-read the item, comments, and labels, then complete the required root-first Dispatch knowledge navigation. Inspect repository files only as needed to verify facts.
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

pub(super) fn normalize_schedule(schedule: String) -> Result<String> {
    let schedule = schedule.trim();
    if schedule.is_empty() {
        bail!("automation schedule is required");
    }
    parse_schedule(schedule)?;
    Ok(schedule.to_owned())
}

pub(crate) fn validate_trigger_configuration(
    name: &str,
    activation: AutomationActivation,
    effect: AutomationEffect,
    schedule: &str,
    work_item_selector: Option<&Condition>,
    prompt: &str,
) -> Result<()> {
    if name.trim().is_empty() {
        bail!("automation trigger name cannot be empty");
    }
    parse_schedule(schedule)?;
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

pub(super) fn default_work_item_selector() -> Condition {
    default_automation_work_item_selector()
}

pub(super) fn default_refinement_work_item_selector() -> Condition {
    needs_refinement_automation_work_item_selector()
}

pub(super) fn default_verification_work_item_selector() -> Condition {
    needs_verification_automation_work_item_selector()
}

pub(crate) fn default_work_item_selector_storage() -> Result<String> {
    selector_to_storage(Some(&default_work_item_selector()))?
        .ok_or_else(|| report!("default work-item automation selector cannot be empty"))
}

pub(super) async fn personality_id_for_effect(
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

pub(super) fn selector_for_activation(
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

pub(crate) fn next_evaluation_at(schedule: &str) -> Result<String> {
    let interval = parse_schedule(schedule)?;
    Ok((OffsetDateTime::now_utc() + interval)
        .format(&Rfc3339)
        .context("failed to format next trigger run time")?)
}

pub(super) fn parse_schedule(schedule: &str) -> Result<Duration> {
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
    let effect = AutomationEffect::from_str(&trigger.effect)?;
    let policy = super::decode_trigger_policy(
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
