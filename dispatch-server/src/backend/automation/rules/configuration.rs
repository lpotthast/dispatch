use crate::backend::items::labels::conditions as label_conditions;
use crudkit_core::condition::Condition;
use dispatch_types::{
    AutomationActivation, AutomationEffect, default_automation_work_item_selector,
    needs_refinement_automation_work_item_selector,
    needs_verification_automation_work_item_selector,
};
use rootcause::{Result, prelude::*};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) const DEFAULT_WORK_ITEM_AUTOMATION_NAME: &str = "Claim open work";
pub(crate) const DEFAULT_REFINEMENT_AUTOMATION_NAME: &str = "Refine needs-refinement work";
pub(crate) const DEFAULT_VERIFICATION_AUTOMATION_NAME: &str = "Verify needs-verification work";
pub(crate) const DEFAULT_WORK_ITEM_AUTOMATION_SCHEDULE: &str = "@every 15s";
pub(crate) const REFINEMENT_AUTOMATION_PRIORITY: i64 = 20;
pub(crate) const VERIFICATION_AUTOMATION_PRIORITY: i64 = 10;

pub(crate) const DEFAULT_REFINEMENT_AUTOMATION_PROMPT: &str = r#"You are the needs-refinement executor for the claimed Dispatch work item.

Goal: turn a rough or under-specified item into implementation-ready work. Do not implement the work.

Required workflow:
- Re-read the item, comments, and labels, then complete the required root-first Dispatch knowledge navigation before editing it.
- Clarify the title and description so a later implementation agent can act without guessing. Prefer concrete scope, non-goals, acceptance criteria, suggested approach, verification expectations, and open questions only when human input is genuinely required.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-refinement` label when refinement is complete. Keep or add `needs-verification` only when the refined item should be checked before implementation.
- Add a concise progress comment summarizing what changed.

Do not call `dispatch item finish` for successful refinement, and do not call `dispatch item release` after successful refinement. Let Dispatch release the temporary claim after your final response. If the item cannot be refined without a human decision, leave `needs-refinement` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user."#;

pub(crate) const DEFAULT_VERIFICATION_AUTOMATION_PROMPT: &str = r#"You are the needs-verification executor for the claimed Dispatch work item.

Goal: verify whether the item is necessary, accurate, and ready for a later implementation agent. Do not implement the work.

Required workflow:
- Re-read the item, comments, and labels, then complete the required root-first Dispatch knowledge navigation. Inspect repository files only as needed to verify facts.
- Update the title or description with verification findings, corrected scope, risks, acceptance criteria, and verification notes that future workers need.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-verification` label when verification is complete. Add `needs-refinement` only if the item still needs story-shaping before implementation.
- Add a concise progress comment with the verification result.

If verification shows the work is unnecessary, explain why in the item and a comment. You may move the item to a project-specific terminal state only when that state already exists in the project's visible workflow vocabulary; do not invent or hardcode a state name. Use `dispatch label suggestions --json`, existing item labels, comments, or project docs to infer that vocabulary.

Do not call `dispatch item finish` for successful verification, and do not call `dispatch item release` after successful verification. Let Dispatch release the temporary claim after your final response. If verification needs a user decision, leave `needs-verification` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user. If verification is blocked by a technical or environment issue rather than missing user input, call `dispatch item release --comment ...` with the blocker."#;

pub(crate) fn normalize_schedule(schedule: String) -> Result<String> {
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
pub(crate) fn default_work_item_selector() -> Condition {
    default_automation_work_item_selector()
}
pub(crate) fn default_refinement_work_item_selector() -> Condition {
    needs_refinement_automation_work_item_selector()
}
pub(crate) fn default_verification_work_item_selector() -> Condition {
    needs_verification_automation_work_item_selector()
}
pub(crate) fn selector_for_activation(
    activation: AutomationActivation,
    selector: Option<Condition>,
) -> Result<Option<Condition>> {
    match (activation, selector) {
        (AutomationActivation::WorkItem, None) => Ok(Some(default_work_item_selector())),
        (_, selector) => Ok(selector),
    }
}
pub(crate) fn next_evaluation_at(schedule: &str) -> Result<String> {
    let interval = parse_schedule(schedule)?;
    Ok((OffsetDateTime::now_utc() + interval)
        .format(&Rfc3339)
        .context("failed to format next trigger run time")?)
}
pub(crate) fn parse_schedule(schedule: &str) -> Result<Duration> {
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
