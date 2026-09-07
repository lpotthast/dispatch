use crate::backend::items::labels::conditions as label_conditions;
use crudkit_core::condition::{Condition, ConditionElement};
use dispatch_types::SelectorClauseResultView;
use rootcause::Result;
pub(super) fn clause_results(
    condition: &Condition,
    labels: &[crate::shared::view_models::WorkItemLabelView],
) -> Result<Vec<SelectorClauseResultView>> {
    let mut results = Vec::new();
    collect_clause_results(condition, labels, "$", &mut results)?;
    Ok(results)
}

fn collect_clause_results(
    condition: &Condition,
    labels: &[crate::shared::view_models::WorkItemLabelView],
    path: &str,
    results: &mut Vec<SelectorClauseResultView>,
) -> Result<()> {
    let elements = match condition {
        Condition::All(elements) | Condition::Any(elements) => elements,
    };
    for (index, element) in elements.iter().enumerate() {
        let element_path = format!("{path}[{index}]");
        match element {
            ConditionElement::Clause(clause) => {
                let condition = Condition::All(vec![ConditionElement::Clause(clause.clone())]);
                let matched =
                    label_conditions::ValidatedLabelCondition::new(&condition)?.matches(labels);
                results.push(SelectorClauseResultView {
                    path: element_path,
                    column_name: Some(clause.column_name.clone()),
                    matched,
                    detail: format!(
                        "{} {:?} {:?}",
                        clause.column_name, clause.operator, clause.value
                    ),
                });
            }
            ConditionElement::Condition(nested) => {
                collect_clause_results(nested, labels, &element_path, results)?;
            }
        }
    }
    Ok(())
}
