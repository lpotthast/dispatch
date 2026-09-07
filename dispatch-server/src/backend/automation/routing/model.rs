use dispatch_types::{WorkItemLabelView, WorkItemSummaryView};

#[derive(Default)]
pub(super) struct RoutingMatchPreview {
    pub(super) matching_item_count: u64,
    pub(super) example_items: Vec<WorkItemSummaryView>,
    pub(super) first_example_labels: Vec<WorkItemLabelView>,
}
