use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::{
    backend::{
        entities::swim_lane::{self, SwimLane, SwimLaneActiveModel, SwimLaneModel},
        label_conditions, projects,
        storage::{Store, utc_now},
    },
    shared::view_models::{STATE_LABEL_KEY, SwimLaneView},
};

const DEFAULT_SWIM_LANES: [(&str, &str, i64, bool); 4] = [
    ("idea", "Idea", 10, true),
    ("open", "Open", 20, true),
    ("in_progress", "In progress", 30, false),
    ("done", "Done", 40, false),
];
pub const DEFAULT_SWIM_LANE_ITEM_ORDER: &str = "updated_desc";

pub async fn list_swim_lanes(store: &Store, project_name: &str) -> Result<Vec<SwimLaneView>> {
    let project_id = projects::project_id(store, project_name).await?;
    list_swim_lanes_for_project_id(store, project_id).await
}

pub async fn list_swim_lanes_for_project_id(
    store: &Store,
    project_id: i64,
) -> Result<Vec<SwimLaneView>> {
    let lanes = SwimLane::find()
        .filter(swim_lane::Column::ProjectId.eq(project_id))
        .order_by_asc(swim_lane::Column::Position)
        .order_by_asc(swim_lane::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to list swim-lanes")?;

    lanes.into_iter().map(model_to_view).collect()
}

pub async fn ensure_default_swim_lanes_for_project_id(
    store: &Store,
    project_id: i64,
) -> Result<()> {
    ensure_default_swim_lanes_in_conn(store.db().as_ref(), project_id).await
}

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
            filter: Set(condition_to_filter_json(&state_filter(identifier))?),
            item_order: Set(DEFAULT_SWIM_LANE_ITEM_ORDER.to_owned()),
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

pub fn normalize_filter_json(filter: impl Into<String>) -> Result<String> {
    let filter = filter.into();
    let filter = filter.trim();
    if filter.is_empty() {
        return condition_to_filter_json(&Condition::all());
    }
    let condition = serde_json::from_str::<Condition>(filter)
        .context("swim-lane filter must be a CrudKit Condition JSON object")?;
    label_conditions::validate_condition(&condition)?;
    condition_to_filter_json(&condition)
}

pub fn normalize_item_order(item_order: impl Into<String>) -> Result<String> {
    let item_order = item_order.into().trim().to_owned();
    match item_order.as_str() {
        "updated_desc" | "updated_asc" | "created_desc" | "created_asc" | "id_desc" | "id_asc"
        | "title_asc" | "title_desc" => Ok(item_order),
        _ => bail!(
            "swim-lane item order must be one of: updated_desc, updated_asc, created_desc, created_asc, id_desc, id_asc, title_asc, title_desc"
        ),
    }
}

pub fn state_filter(state: &str) -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: STATE_LABEL_KEY.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::String(state.to_owned()),
    })])
}

fn condition_to_filter_json(condition: &Condition) -> Result<String> {
    Ok(serde_json::to_string(condition).context("failed to serialize swim-lane filter")?)
}

fn model_to_view(model: SwimLaneModel) -> Result<SwimLaneView> {
    let filter = serde_json::from_str::<Condition>(&model.filter)
        .context_with(|| format!("failed to parse swim-lane '{}' filter", model.identifier))?;
    Ok(SwimLaneView {
        id: model.id,
        project_id: model.project_id,
        identifier: model.identifier,
        name: model.name,
        position: model.position,
        filter,
        item_order: model.item_order,
        can_create_items: model.can_create_items,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::backend::projects::{CreateProject, create_project};

    #[tokio::test]
    async fn default_swim_lanes_configure_item_creation() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();

        let flags = list_swim_lanes(&store, "demo")
            .await
            .unwrap()
            .into_iter()
            .map(|lane| (lane.identifier, lane.can_create_items))
            .collect::<Vec<_>>();

        assert_eq!(
            flags,
            vec![
                ("idea".to_owned(), true),
                ("open".to_owned(), true),
                ("in_progress".to_owned(), false),
                ("done".to_owned(), false),
            ]
        );
    }

    #[tokio::test]
    async fn default_swim_lanes_filter_by_matching_state() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();

        let lanes = list_swim_lanes(&store, "demo")
            .await
            .unwrap()
            .into_iter()
            .map(|lane| (lane.identifier, lane.filter, lane.item_order))
            .collect::<Vec<_>>();

        assert_eq!(
            lanes,
            vec![
                (
                    "idea".to_owned(),
                    state_filter("idea"),
                    DEFAULT_SWIM_LANE_ITEM_ORDER.to_owned()
                ),
                (
                    "open".to_owned(),
                    state_filter("open"),
                    DEFAULT_SWIM_LANE_ITEM_ORDER.to_owned()
                ),
                (
                    "in_progress".to_owned(),
                    state_filter("in_progress"),
                    DEFAULT_SWIM_LANE_ITEM_ORDER.to_owned()
                ),
                (
                    "done".to_owned(),
                    state_filter("done"),
                    DEFAULT_SWIM_LANE_ITEM_ORDER.to_owned()
                ),
            ]
        );
    }
}
