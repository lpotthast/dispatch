use super::model::StateFields;
use crate::backend::{
    entities::work_item_state::{
        self, WorkItemState, WorkItemStateActiveModel, WorkItemStateModel,
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::WorkItemStateView;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

const DEFAULT_WORK_ITEM_STATES: [(&str, &str, i64); 4] = [
    ("idea", "Idea", 10),
    ("open", "Open", 20),
    ("in_progress", "In progress", 30),
    ("done", "Done", 40),
];
pub(crate) async fn ensure_default_work_item_states_in_conn<C>(
    conn: &C,
    project_id: i64,
) -> Result<()>
where
    C: sea_orm::ConnectionTrait,
{
    for (identifier, name, position) in DEFAULT_WORK_ITEM_STATES {
        if WorkItemState::find()
            .filter(work_item_state::Column::ProjectId.eq(project_id))
            .filter(work_item_state::Column::Identifier.eq(identifier))
            .one(conn)
            .await
            .context_with(|| format!("failed to check work item state '{identifier}'"))?
            .is_some()
        {
            continue;
        }

        let now = utc_now();
        let active = WorkItemStateActiveModel {
            project_id: Set(project_id),
            identifier: Set(identifier.to_owned()),
            name: Set(name.to_owned()),
            position: Set(position),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        };
        active
            .insert(conn)
            .await
            .context_with(|| format!("failed to create work item state '{identifier}'"))?;
    }
    Ok(())
}

fn decode(model: WorkItemStateModel) -> Result<WorkItemStateView> {
    Ok(WorkItemStateView {
        id: model.id,
        project_id: model.project_id,
        identifier: model.identifier,
        name: model.name,
        position: model.position,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

pub(crate) fn encode(record: WorkItemStateView) -> Result<work_item_state::Model> {
    Ok(work_item_state::Model {
        id: record.id,
        project_id: record.project_id,
        identifier: record.identifier,
        name: record.name,
        position: record.position,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}
pub(crate) struct StateRepository;
impl StateRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<WorkItemStateView>> {
        WorkItemState::find()
            .filter(work_item_state::Column::ProjectId.eq(project_id))
            .order_by_asc(work_item_state::Column::Position)
            .order_by_asc(work_item_state::Column::Id)
            .all(transaction.connection())
            .await
            .context("failed to list states")?
            .into_iter()
            .map(decode)
            .collect()
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<WorkItemStateView> {
        let row = WorkItemState::find_by_id(id)
            .filter(work_item_state::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load states record")?
            .ok_or_else(|| report!("states record {id} does not exist in this project"))?;
        decode(row)
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        input: StateFields,
    ) -> Result<WorkItemStateView> {
        let now = utc_now();
        let row = WorkItemStateActiveModel {
            project_id: Set(project_id),
            identifier: Set(input.identifier),
            name: Set(input.name),
            position: Set(input.position),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(transaction.connection())
        .await
        .context("failed to create states record")?;
        decode(row)
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: WorkItemStateView,
    ) -> Result<WorkItemStateView> {
        use sea_orm::IntoActiveModel;
        let row = encode(record)?
            .into_active_model()
            .reset_all()
            .update(transaction.connection())
            .await
            .context("failed to update states record")?;
        decode(row)
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<u64> {
        Ok(WorkItemState::delete_many()
            .filter(work_item_state::Column::ProjectId.eq(project_id))
            .filter(work_item_state::Column::Id.eq(id))
            .exec(transaction.connection())
            .await
            .context("failed to delete states record")?
            .rows_affected)
    }
}
