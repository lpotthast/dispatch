use super::model::SemanticEvaluation;
use crate::backend::{
    items::labels::conditions as label_conditions, items::labels::policy as item_labels,
};
use dispatch_types::{
    AgentCommitOutcome, AutomationPostconditions, ExpectedDisposition, LabelAssertion,
    LabelAssertionKind, PostconditionFailureView, SemanticPostconditionStatus, WorkItemEventType,
    WorkItemEventView, WorkItemView, WorkspaceAssertion,
};
use rootcause::{Result, prelude::*};
use std::collections::BTreeSet;
pub(crate) fn validate_configuration(
    postconditions: &AutomationPostconditions,
    path: &str,
) -> Result<()> {
    if postconditions.any_of.is_empty() {
        bail!("{path}.any_of cannot be empty");
    }
    for (outcome_index, outcome) in postconditions.any_of.iter().enumerate() {
        for label in &outcome.labels {
            item_labels::normalize_key(label.key.clone())
                .context_with(|| format!("{path}.any_of[{outcome_index}] has invalid label"))?;
        }
        if let Some(created) = &outcome.created_items {
            validate_created_item_assertion(
                created,
                &format!("{path}.any_of[{outcome_index}].created_items"),
            )?;
        }
        for (assertion_index, created) in outcome.created_item_assertions.iter().enumerate() {
            validate_created_item_assertion(
                created,
                &format!(
                    "{path}.any_of[{outcome_index}].created_item_assertions[{assertion_index}]"
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_created_item_assertion(
    created: &crate::shared::view_models::CreatedItemAssertion,
    path: &str,
) -> Result<()> {
    if created.count.is_some() && (created.at_least.is_some() || created.at_most.is_some()) {
        bail!("{path}.count cannot be combined with at_least or at_most");
    }
    if let (Some(minimum), Some(maximum)) = (created.at_least, created.at_most)
        && minimum > maximum
    {
        bail!("{path} has at_least greater than at_most");
    }
    if let Some(selector) = &created.selector {
        crate::backend::items::labels::conditions::validate_condition(selector)?;
    }
    Ok(())
}

pub(super) fn evaluate(
    baseline_item: Option<&WorkItemView>,
    current_item: Option<&WorkItemView>,
    events: &[WorkItemEventView],
    created_items: &[WorkItemView],
    postconditions: &AutomationPostconditions,
    commit_outcome: AgentCommitOutcome,
) -> Result<SemanticEvaluation> {
    let event_types = events
        .iter()
        .map(|event| event.event_type)
        .collect::<BTreeSet<_>>();
    let workspace_changed = !matches!(
        commit_outcome,
        AgentCommitOutcome::SkippedNoChanges | AgentCommitOutcome::SkippedNoGitRepo
    );

    let mut all_failures = Vec::new();
    for (outcome_index, outcome) in postconditions.any_of.iter().enumerate() {
        let mut failures = Vec::new();
        if let Some(expected) = outcome.disposition {
            let actual = actual_disposition(&event_types);
            if actual != expected {
                failures.push(failure(
                    outcome_index,
                    "disposition",
                    format!("{expected:?}"),
                    format!("{actual:?}"),
                ));
            }
        }
        for expected in &outcome.attributed_events {
            if !event_types.contains(expected) {
                failures.push(failure(
                    outcome_index,
                    "attributed_event",
                    expected.as_storage(),
                    "missing",
                ));
            }
        }
        for assertion in &outcome.labels {
            evaluate_label_assertion(
                outcome_index,
                assertion,
                baseline_item,
                current_item,
                events,
                &mut failures,
            );
        }
        if let Some(assertion) = &outcome.created_items {
            evaluate_created_item_assertion(
                outcome_index,
                "created_items",
                assertion,
                created_items,
                &mut failures,
            )?;
        }
        for (assertion_index, assertion) in outcome.created_item_assertions.iter().enumerate() {
            evaluate_created_item_assertion(
                outcome_index,
                &format!("created_item_assertions[{assertion_index}]"),
                assertion,
                created_items,
                &mut failures,
            )?;
        }
        if outcome.created_items_share_group {
            let group_ids = created_items
                .iter()
                .filter_map(|item| item.work_group.as_ref().map(|group| group.id))
                .collect::<BTreeSet<_>>();
            if created_items.is_empty()
                || group_ids.len() != 1
                || created_items.iter().any(|item| item.work_group.is_none())
            {
                failures.push(failure(
                    outcome_index,
                    "created_items_share_group",
                    "all run-created items assigned to one work group",
                    format!(
                        "created_items={}, assigned_items={}, distinct_groups={}",
                        created_items.len(),
                        created_items
                            .iter()
                            .filter(|item| item.work_group.is_some())
                            .count(),
                        group_ids.len()
                    ),
                ));
            }
        }
        if let Some(assertion) = outcome.workspace_changes {
            let passed = match assertion {
                WorkspaceAssertion::Any => true,
                WorkspaceAssertion::None => !workspace_changed,
                WorkspaceAssertion::Required => workspace_changed,
            };
            if !passed {
                failures.push(failure(
                    outcome_index,
                    "workspace_changes",
                    format!("{assertion:?}"),
                    workspace_changed.to_string(),
                ));
            }
        }
        if failures.is_empty() {
            return Ok(SemanticEvaluation {
                status: SemanticPostconditionStatus::Passed,
                failures: Vec::new(),
            });
        }
        all_failures.extend(failures);
    }

    Ok(SemanticEvaluation {
        status: SemanticPostconditionStatus::Failed,
        failures: all_failures,
    })
}
fn evaluate_created_item_assertion(
    outcome_index: usize,
    assertion_name: &str,
    assertion: &crate::shared::view_models::CreatedItemAssertion,
    created_items: &[WorkItemView],
    failures: &mut Vec<PostconditionFailureView>,
) -> Result<()> {
    let matching = match &assertion.selector {
        Some(selector) => {
            let selector = label_conditions::ValidatedLabelCondition::new(selector)?;
            created_items
                .iter()
                .filter(|item| selector.matches(&item.labels))
                .count() as u64
        }
        None => created_items.len() as u64,
    };
    if let Some(count) = assertion.count
        && matching != count
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.count"),
            count.to_string(),
            matching.to_string(),
        ));
    }
    if let Some(minimum) = assertion.at_least
        && matching < minimum
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.at_least"),
            minimum.to_string(),
            matching.to_string(),
        ));
    }
    if let Some(maximum) = assertion.at_most
        && matching > maximum
    {
        failures.push(failure(
            outcome_index,
            format!("{assertion_name}.at_most"),
            maximum.to_string(),
            matching.to_string(),
        ));
    }
    Ok(())
}

fn actual_disposition(events: &BTreeSet<WorkItemEventType>) -> ExpectedDisposition {
    if events.contains(&WorkItemEventType::ItemFinished) {
        ExpectedDisposition::Finished
    } else if events.contains(&WorkItemEventType::FeedbackRequested) {
        ExpectedDisposition::FeedbackRequested
    } else if events.contains(&WorkItemEventType::ItemReleased) {
        ExpectedDisposition::Released
    } else {
        ExpectedDisposition::SuccessfulNonterminal
    }
}

fn evaluate_label_assertion(
    outcome_index: usize,
    assertion: &LabelAssertion,
    baseline: Option<&WorkItemView>,
    current: Option<&WorkItemView>,
    events: &[WorkItemEventView],
    failures: &mut Vec<PostconditionFailureView>,
) {
    let before = baseline.is_some_and(|item| has_label(item, assertion));
    let after = current.is_some_and(|item| has_label(item, assertion));
    let attributed_transition = baseline.is_some_and(|item| {
        events
            .iter()
            .any(|event| label_event_matches(event, item.id, assertion))
    });
    let passed = match assertion.assertion {
        LabelAssertionKind::Added => !before && after && attributed_transition,
        LabelAssertionKind::Removed => before && !after && attributed_transition,
        LabelAssertionKind::Present => after,
        LabelAssertionKind::Absent => !after,
    };
    if !passed {
        failures.push(failure(
            outcome_index,
            format!("label_{:?}", assertion.assertion).to_lowercase(),
            match assertion.value.as_deref() {
                Some(value) => format!("{}={value}", assertion.key),
                None => assertion.key.clone(),
            },
            format!("before={before}, after={after}"),
        ));
    }
}

fn label_event_matches(
    event: &WorkItemEventView,
    item_id: i64,
    assertion: &LabelAssertion,
) -> bool {
    if event.work_item_id != Some(item_id) {
        return false;
    }
    let expected = item_labels::format_label(&assertion.key, assertion.value.as_deref());
    match assertion.assertion {
        LabelAssertionKind::Added => {
            (event.event_type == WorkItemEventType::LabelAdded
                && event.body == format!("Added label {expected}"))
                || (event.event_type == WorkItemEventType::LabelUpdated
                    && event.body == format!("Updated label {expected}"))
        }
        LabelAssertionKind::Removed => {
            (event.event_type == WorkItemEventType::LabelDeleted
                && event.body == format!("Deleted label {expected}"))
                || event.event_type == WorkItemEventType::LabelUpdated
        }
        LabelAssertionKind::Present | LabelAssertionKind::Absent => true,
    }
}

fn has_label(item: &WorkItemView, assertion: &LabelAssertion) -> bool {
    item.labels.iter().any(|label| {
        label.key == assertion.key
            && assertion
                .value
                .as_ref()
                .is_none_or(|value| label.value.as_ref() == Some(value))
    })
}

fn failure(
    outcome_index: usize,
    assertion: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> PostconditionFailureView {
    PostconditionFailureView {
        outcome_index,
        assertion: assertion.into(),
        expected: expected.into(),
        actual: actual.into(),
    }
}
