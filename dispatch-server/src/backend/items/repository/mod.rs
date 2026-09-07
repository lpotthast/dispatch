mod mutations;
mod queries;
pub(crate) mod records;
mod search;
pub(crate) mod views;

use crate::{backend::storage::Transaction, shared::view_models::WorkItemView};
use rootcause::Result;
pub(crate) struct ItemRepository;
impl ItemRepository {
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<WorkItemView> {
        let row = records::get(transaction.connection(), project_id, item_id).await?;
        views::model_to_view(transaction.connection(), row).await
    }
}
