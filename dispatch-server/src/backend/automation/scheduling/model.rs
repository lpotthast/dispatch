use dispatch_types::AutomationTriggerView;
pub(crate) enum RuleSelection {
    Enabled,
    Queued,
    WorkItems(i64),
}
#[derive(Clone, Copy)]
pub(crate) enum ScheduleChange {
    Evaluated,
    Checked,
    Cursor(Option<i64>),
}
pub(crate) struct CreatedItemEvent {
    pub(crate) id: i64,
    pub(crate) work_item_id: Option<i64>,
}
pub(crate) struct WorkItemAutomationCandidate {
    pub(crate) view: AutomationTriggerView,
    pub(crate) item_ids: Vec<i64>,
}
