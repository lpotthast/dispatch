use super::model::*;
use crate::backend::items::labels::workflow as workflow_labels;
use dispatch_types::{WorkItemEventType, WorkItemView};
use rootcause::{Result, prelude::*};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClaimReturnMode<'a> {
    Release {
        comment: Option<&'a str>,
        automation_disposition: ReleaseAutomationDisposition,
    },
    FeedbackRequest {
        body: &'a str,
    },
}

impl ClaimReturnMode<'_> {
    pub(super) fn agent_comment_body(&self) -> Option<&str> {
        match self {
            Self::Release { comment, .. } => *comment,
            Self::FeedbackRequest { body } => Some(*body),
        }
    }

    pub(super) fn label_disposition(&self) -> workflow_labels::ClaimReturnLabelDisposition {
        match self {
            Self::Release {
                automation_disposition,
                ..
            } => match automation_disposition {
                ReleaseAutomationDisposition::Claimable => {
                    workflow_labels::ClaimReturnLabelDisposition::ClaimableRelease
                }
                ReleaseAutomationDisposition::Blocked => {
                    workflow_labels::ClaimReturnLabelDisposition::BlockedRelease
                }
            },
            Self::FeedbackRequest { .. } => {
                workflow_labels::ClaimReturnLabelDisposition::FeedbackRequest
            }
        }
    }

    pub(super) fn update_context(&self) -> &'static str {
        match self {
            Self::Release { .. } => "failed to release work item",
            Self::FeedbackRequest { .. } => "failed to request item feedback",
        }
    }

    pub(super) fn start_context(&self) -> &'static str {
        match self {
            Self::Release { .. } => "failed to start item release",
            Self::FeedbackRequest { .. } => "failed to start feedback request",
        }
    }

    pub(super) fn event_type(&self) -> WorkItemEventType {
        match self {
            Self::Release { .. } => WorkItemEventType::ItemReleased,
            Self::FeedbackRequest { .. } => WorkItemEventType::FeedbackRequested,
        }
    }

    pub(super) fn event_body(&self, agent_id: &str, release_state: &str) -> String {
        match self {
            Self::Release { .. } => {
                format!("Released by {agent_id}; restored state to {release_state}")
            }
            Self::FeedbackRequest { .. } => {
                format!("Feedback requested by {agent_id}; restored state to {release_state}")
            }
        }
    }

    pub(super) fn commit_context(&self) -> &'static str {
        match self {
            Self::Release { .. } => "failed to commit item release",
            Self::FeedbackRequest { .. } => "failed to commit feedback request",
        }
    }
}

impl AutomationClaimOutcome {
    pub(super) fn release_disposition(self) -> ReleaseAutomationDisposition {
        match self {
            Self::CompletedUnfinished | Self::Cancelled => ReleaseAutomationDisposition::Claimable,
            Self::Failed => ReleaseAutomationDisposition::Blocked,
        }
    }

    pub(super) fn release_comment_base(self) -> &'static str {
        match self {
            Self::CompletedUnfinished => {
                "Automation turn completed without finishing the item; releasing claim."
            }
            Self::Failed => "Automation turn failed before finishing the item; releasing claim.",
            Self::Cancelled => {
                "Automation turn was cancelled before finishing the item; releasing claim."
            }
        }
    }
}

pub(super) fn automation_claim_release_comment(
    outcome: AutomationClaimOutcome,
    run_id: i64,
    detail: Option<&str>,
) -> String {
    let mut comment = format!("{} Run #{run_id}.", outcome.release_comment_base());
    if let Some(detail) = detail
        .map(summarize_text)
        .filter(|detail| !detail.is_empty())
    {
        comment.push(' ');
        comment.push_str(&detail);
    }
    comment
}

pub(super) fn summarize_text(value: &str) -> String {
    let mut summary = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned();
    if summary.len() > 4000 {
        let mut end = 4000;
        while end > 0 && !summary.is_char_boundary(end) {
            end -= 1;
        }
        summary.truncate(end);
        summary.push_str("...");
    }
    summary
}

pub(super) fn ensure_active_claim(item: &WorkItemView, agent_id: &str) -> Result<()> {
    match item.claimed_by.as_deref() {
        Some(owner) if owner == agent_id => Ok(()),
        Some(owner) => bail!("item {} is claimed by {owner}", item.id),
        None => bail!("item {} is not claimed", item.id),
    }
}
pub(super) fn clear_claim(item: &mut WorkItemView, now: &str) {
    item.claimed_by = None;
    item.claimed_at = None;
    item.claim_expires_at = None;
    touch_claim(item, now);
}
pub(super) fn touch_claim(item: &mut WorkItemView, now: &str) {
    item.version += 1;
    item.updated_at = now.to_owned();
}
pub(super) fn is_stale(
    item: &WorkItemView,
    stale_after_minutes: i64,
    now: time::OffsetDateTime,
) -> bool {
    let before = |value: &str, cutoff: time::OffsetDateTime| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .map(|timestamp| timestamp <= cutoff)
            .unwrap_or(false)
    };
    match item.claim_expires_at.as_deref() {
        Some(expires) => before(expires, now),
        None => item.claimed_at.as_deref().is_some_and(|claimed| {
            before(claimed, now - time::Duration::minutes(stale_after_minutes))
        }),
    }
}
