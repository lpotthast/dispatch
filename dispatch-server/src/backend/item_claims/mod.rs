mod active_claims;
mod claim_candidates;
mod claim_returns;
mod claiming;
mod progress_finish;
mod stale_recovery;

pub(crate) use claim_returns::{
    AutomationClaimFinalization, AutomationClaimOutcome, finalize_automation_claim,
};
pub(crate) use claim_returns::{ReleaseAutomationDisposition, release_item, request_feedback};
#[cfg(test)]
pub(crate) use claiming::claim_specific_item;
pub(crate) use claiming::{
    claim_item, has_claimable_item_matching_condition,
    has_claimable_specific_item_matching_condition, resolve_agent_run_target,
};
pub(crate) use progress_finish::{finish_item, progress_item};
pub(crate) use stale_recovery::recover_stale_claims;

#[cfg(test)]
mod tests;
