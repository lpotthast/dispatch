use dispatch_types::AgentRunStatus;
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemRunPreview {
    pub id: i64,
    pub status: AgentRunStatus,
    pub result_summary: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ItemRunPreviews {
    pub total: usize,
    pub latest: Vec<ItemRunPreview>,
}

pub(crate) enum RunFilter {
    Project,
    Item(i64),
    Trigger(i64),
}
