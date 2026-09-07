use dispatch_types::AutomationTriggerView;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) const PRIORITY_SCORE_SECONDS: i64 = 300;
const EVALUATION_COUNT_SCORE_SECONDS: i64 = 300;
const NEVER_RUN_SCORE_SECONDS: i64 = 24 * 60 * 60;

pub(crate) fn trigger_due(trigger: &AutomationTriggerView) -> bool {
    trigger
        .next_evaluation_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .is_none_or(|next| next <= OffsetDateTime::now_utc())
}

pub(crate) fn fairness_score(
    automation: &AutomationTriggerView,
    max_evaluation_count: i64,
    now: OffsetDateTime,
) -> i64 {
    let age_seconds = automation
        .last_evaluated_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .map(|last| (now - last).whole_seconds().max(0))
        .unwrap_or(NEVER_RUN_SCORE_SECONDS);
    age_seconds
        .saturating_add(
            max_evaluation_count
                .saturating_sub(automation.evaluation_count)
                .saturating_mul(EVALUATION_COUNT_SCORE_SECONDS),
        )
        .saturating_add(automation.priority.saturating_mul(PRIORITY_SCORE_SECONDS))
}
