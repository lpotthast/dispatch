use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use dispatch_types::{STATE_LABEL_KEY, SwimLaneItemOrder};
use rootcause::{Result, prelude::*};
pub(crate) const DEFAULT_SWIM_LANE_ITEM_ORDER: SwimLaneItemOrder = SwimLaneItemOrder::UpdatedDesc;
pub fn normalize_identifier(identifier: impl Into<String>) -> Result<String> {
    let identifier = identifier.into().trim().to_owned();
    if identifier.is_empty() {
        bail!("swim-lane identifier cannot be empty");
    }
    if identifier.contains('=') {
        bail!("swim-lane identifier cannot contain '='");
    }
    Ok(identifier)
}

pub fn normalize_name(name: impl Into<String>) -> Result<String> {
    let name = name.into().trim().to_owned();
    if name.is_empty() {
        bail!("swim-lane name cannot be empty");
    }
    Ok(name)
}

pub fn parse_item_order(item_order: &str) -> Result<SwimLaneItemOrder> {
    item_order.parse().map_err(Into::into)
}

pub fn state_filter(state: &str) -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String(state.to_owned()),
    })])
}
