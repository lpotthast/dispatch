use super::model::LaneFields;
use crate::backend::{
    entities::swim_lane::{self, SwimLane, SwimLaneActiveModel, SwimLaneModel},
    storage::{Transaction, utc_now},
};
use crudkit_core::condition::Condition;
use dispatch_types::SwimLaneView;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
fn encode_filter(condition: &Condition) -> Result<String> {
    Ok(serde_json::to_string(condition).context("failed to serialize swim-lane filter")?)
}

const DEFAULT_SWIM_LANES: [(&str, &str, i64, bool); 4] = [
    ("idea", "Idea", 10, true),
    ("open", "Open", 20, true),
    ("in_progress", "In progress", 30, false),
    ("done", "Done", 40, false),
];
use super::policy::{DEFAULT_SWIM_LANE_ITEM_ORDER, state_filter};

pub(crate) async fn ensure_default_swim_lanes_in_conn<C>(conn: &C, project_id: i64) -> Result<()>
where
    C: sea_orm::ConnectionTrait,
{
    for (identifier, name, position, can_create_items) in DEFAULT_SWIM_LANES {
        if SwimLane::find()
            .filter(swim_lane::Column::ProjectId.eq(project_id))
            .filter(swim_lane::Column::Identifier.eq(identifier))
            .one(conn)
            .await
            .context_with(|| format!("failed to check swim-lane '{identifier}'"))?
            .is_some()
        {
            continue;
        }

        let now = utc_now();
        let active = SwimLaneActiveModel {
            project_id: Set(project_id),
            identifier: Set(identifier.to_owned()),
            name: Set(name.to_owned()),
            position: Set(position),
            filter: Set(encode_filter(&state_filter(identifier))?),
            item_order: Set(DEFAULT_SWIM_LANE_ITEM_ORDER.as_storage().to_owned()),
            can_create_items: Set(can_create_items),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        };
        active
            .insert(conn)
            .await
            .context_with(|| format!("failed to create swim-lane '{identifier}'"))?;
    }
    Ok(())
}

fn decode(model: SwimLaneModel) -> Result<SwimLaneView> {
    let filter = serde_json::from_str::<Condition>(&model.filter)
        .context_with(|| format!("failed to parse swim-lane '{}' filter", model.identifier))?;
    let item_order = super::policy::parse_item_order(&model.item_order).context_with(|| {
        format!(
            "failed to parse swim-lane '{}' item order",
            model.identifier
        )
    })?;
    crate::backend::items::labels::conditions::validate_condition(&filter)?;
    Ok(SwimLaneView {
        id: model.id,
        project_id: model.project_id,
        identifier: model.identifier,
        name: model.name,
        position: model.position,
        filter,
        item_order,
        can_create_items: model.can_create_items,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

pub(crate) fn encode(record: SwimLaneView) -> Result<swim_lane::Model> {
    Ok(swim_lane::Model {
        id: record.id,
        project_id: record.project_id,
        identifier: record.identifier,
        name: record.name,
        position: record.position,
        filter: encode_filter(&record.filter)?,
        item_order: record.item_order.as_storage().to_owned(),
        can_create_items: record.can_create_items,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}
pub(crate) struct LaneRepository;
impl LaneRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<SwimLaneView>> {
        SwimLane::find()
            .filter(swim_lane::Column::ProjectId.eq(project_id))
            .order_by_asc(swim_lane::Column::Position)
            .order_by_asc(swim_lane::Column::Id)
            .all(transaction.connection())
            .await
            .context("failed to list lanes")?
            .into_iter()
            .map(decode)
            .collect()
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<SwimLaneView> {
        let row = SwimLane::find_by_id(id)
            .filter(swim_lane::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load lanes record")?
            .ok_or_else(|| report!("lanes record {id} does not exist in this project"))?;
        decode(row)
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        input: LaneFields,
    ) -> Result<SwimLaneView> {
        let now = utc_now();
        let row = SwimLaneActiveModel {
            project_id: Set(project_id),
            identifier: Set(input.identifier),
            name: Set(input.name),
            position: Set(input.position),
            filter: Set(encode_filter(&input.filter)?),
            item_order: Set(input.item_order.as_storage().to_owned()),
            can_create_items: Set(input.can_create_items),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(transaction.connection())
        .await
        .context("failed to create lanes record")?;
        decode(row)
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: SwimLaneView,
    ) -> Result<SwimLaneView> {
        use sea_orm::IntoActiveModel;
        let row = encode(record)?
            .into_active_model()
            .reset_all()
            .update(transaction.connection())
            .await
            .context("failed to update lanes record")?;
        decode(row)
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<u64> {
        Ok(SwimLane::delete_many()
            .filter(swim_lane::Column::ProjectId.eq(project_id))
            .filter(swim_lane::Column::Id.eq(id))
            .exec(transaction.connection())
            .await
            .context("failed to delete lanes record")?
            .rows_affected)
    }
}
