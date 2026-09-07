use super::model::RoutingMatchPreview;
use crate::backend::{
    entities::work_item::{self, WorkItem},
    items::labels::{
        conditions::ValidatedLabelCondition,
        repository::{conditions::matching_automation_items, records as labels},
        workflow::current_state,
    },
    storage::Transaction,
};
use dispatch_types::WorkItemSummaryView;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};

const EXAMPLE_LIMIT: u64 = 10;

#[derive(FromQueryResult)]
struct ExampleItem {
    id: i64,
    title: String,
    updated_at: String,
}

pub(crate) struct RoutingRepository;

impl RoutingRepository {
    pub(super) async fn preview_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        condition: &ValidatedLabelCondition,
    ) -> Result<RoutingMatchPreview> {
        let query = WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(matching_automation_items(project_id, condition));
        let matching_item_count = query
            .clone()
            .count(transaction.connection())
            .await
            .context("failed to count routing matches")?;
        if matching_item_count == 0 {
            return Ok(RoutingMatchPreview::default());
        }
        let items = query
            .select_only()
            .columns([
                work_item::Column::Id,
                work_item::Column::Title,
                work_item::Column::UpdatedAt,
            ])
            .order_by_desc(work_item::Column::UpdatedAt)
            .order_by_desc(work_item::Column::Id)
            .limit(EXAMPLE_LIMIT)
            .into_model::<ExampleItem>()
            .all(transaction.connection())
            .await
            .context("failed to load routing examples")?;
        let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
        let mut labels = labels::for_items(transaction.connection(), project_id, &item_ids).await?;
        let mut preview = RoutingMatchPreview {
            matching_item_count,
            ..Default::default()
        };
        for item in items {
            let item_labels = labels.remove(&item.id).unwrap_or_default();
            let state = current_state(&item_labels);
            if preview.example_items.is_empty() {
                preview.first_example_labels = item_labels;
            }
            preview.example_items.push(WorkItemSummaryView {
                id: item.id,
                title: item.title,
                state,
                updated_at: item.updated_at,
            });
        }
        Ok(preview)
    }
}
