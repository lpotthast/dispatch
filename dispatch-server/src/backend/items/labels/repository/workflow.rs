use crate::backend::items::labels::{
    repository::records as work_item_labels, workflow::WorkflowLabelPlan,
};
use rootcause::Result;
use sea_orm::ConnectionTrait;
pub(crate) async fn apply_plan<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    plan: WorkflowLabelPlan<'_>,
) -> Result<()>
where
    C: ConnectionTrait,
{
    for label_key in plan.delete_keys {
        work_item_labels::delete_by_key_in_tx(conn, project_id, item_id, label_key).await?;
    }
    for upsert in plan.upserts {
        work_item_labels::upsert_in_tx(conn, project_id, item_id, upsert.key, upsert.value).await?;
    }
    Ok(())
}
