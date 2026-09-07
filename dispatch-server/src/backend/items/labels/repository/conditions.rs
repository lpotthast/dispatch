use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QuerySelect, QueryTrait, sea_query::Expr,
};

use crate::backend::items::labels::workflow::AUTOMATION_BLOCKING_LABEL_KEYS;
use crate::{
    backend::entities::{
        work_item,
        work_item_label::{self, WorkItemLabel},
    },
    shared::label_conditions::{
        LabelClause, LabelCondition, LabelPredicate, ValidatedLabelCondition,
    },
};
use dispatch_types::STATE_LABEL_KEY;

/// Selects item IDs using the same validated condition as in-memory matching.
pub(crate) fn matching_items(project_id: i64, condition: &ValidatedLabelCondition) -> Condition {
    translate(project_id, condition.condition())
}

pub(crate) fn matching_automation_items(
    project_id: i64,
    condition: &ValidatedLabelCondition,
) -> Condition {
    Condition::all()
        .add(matching_items(project_id, condition))
        .add(unblocked_items(project_id))
}

pub(crate) fn unblocked_items(project_id: i64) -> Condition {
    let blocked = WorkItemLabel::find()
        .select_only()
        .column(work_item_label::Column::WorkItemId)
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::Key.is_in(AUTOMATION_BLOCKING_LABEL_KEYS.iter().copied()));
    Condition::all().add(work_item::Column::Id.not_in_subquery(blocked.into_query()))
}

pub(crate) fn items_in_states<'a>(
    project_id: i64,
    states: impl IntoIterator<Item = &'a str>,
) -> Condition {
    let labels = WorkItemLabel::find()
        .select_only()
        .column(work_item_label::Column::WorkItemId)
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::Key.eq(STATE_LABEL_KEY))
        .filter(work_item_label::Column::Value.is_in(states));
    Condition::all().add(work_item::Column::Id.in_subquery(labels.into_query()))
}

fn translate(project_id: i64, condition: &LabelCondition) -> Condition {
    match condition {
        LabelCondition::All(elements) | LabelCondition::Any(elements) => {
            let all = matches!(condition, LabelCondition::All(_));
            // SeaQuery omits empty groups; label conditions give them Boolean identities.
            if elements.is_empty() {
                return Condition::all().add(Expr::val(all).eq(true));
            }
            elements.iter().fold(
                if all {
                    Condition::all()
                } else {
                    Condition::any()
                },
                |result, element| result.add(translate(project_id, element)),
            )
        }
        LabelCondition::Clause(LabelClause {
            key,
            predicate,
            negated,
        }) => {
            let mut labels = WorkItemLabel::find()
                .select_only()
                .column(work_item_label::Column::WorkItemId)
                .filter(work_item_label::Column::ProjectId.eq(project_id))
                .filter(work_item_label::Column::Key.eq(key));
            labels = match predicate {
                LabelPredicate::Present => labels,
                LabelPredicate::ValueEquals(expected) => {
                    labels.filter(work_item_label::Column::Value.eq(expected))
                }
                LabelPredicate::ValueIsNull => {
                    labels.filter(work_item_label::Column::Value.is_null())
                }
                LabelPredicate::ValueIn(expected) => {
                    labels.filter(work_item_label::Column::Value.is_in(expected.clone()))
                }
            };
            // Negate membership, not the nullable value comparison: missing labels
            // satisfy inequality, and a flag label is distinct from an absent label.
            Condition::all().add(if *negated {
                work_item::Column::Id.not_in_subquery(labels.into_query())
            } else {
                work_item::Column::Id.in_subquery(labels.into_query())
            })
        }
    }
}
