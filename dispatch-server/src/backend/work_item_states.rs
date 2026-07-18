use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::{
    backend::{
        entities::work_item_state::{
            self, WorkItemState, WorkItemStateActiveModel, WorkItemStateModel,
        },
        projects,
        storage::{Store, utc_now},
    },
    shared::view_models::WorkItemStateView,
};

const DEFAULT_WORK_ITEM_STATES: [(&str, &str, i64); 4] = [
    ("idea", "Idea", 10),
    ("open", "Open", 20),
    ("in_progress", "In progress", 30),
    ("done", "Done", 40),
];

pub async fn list_work_item_states(
    store: &Store,
    project_name: &str,
) -> Result<Vec<WorkItemStateView>> {
    let project_id = projects::project_id(store, project_name).await?;
    list_work_item_states_for_project_id(store, project_id).await
}

pub async fn list_work_item_states_for_project_id(
    store: &Store,
    project_id: i64,
) -> Result<Vec<WorkItemStateView>> {
    let states = WorkItemState::find()
        .filter(work_item_state::Column::ProjectId.eq(project_id))
        .order_by_asc(work_item_state::Column::Position)
        .order_by_asc(work_item_state::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to list work item states")?;

    Ok(states.into_iter().map(model_to_view).collect())
}

pub async fn ensure_default_work_item_states_for_project_id(
    store: &Store,
    project_id: i64,
) -> Result<()> {
    ensure_default_work_item_states_in_conn(store.db().as_ref(), project_id).await
}

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

pub fn normalize_identifier(identifier: impl Into<String>) -> Result<String> {
    let identifier = identifier.into().trim().to_owned();
    if identifier.is_empty() {
        bail!("work item state identifier cannot be empty");
    }
    if identifier.contains('=') {
        bail!("work item state identifier cannot contain '='");
    }
    Ok(identifier)
}

pub fn normalize_name(name: impl Into<String>) -> Result<String> {
    let name = name.into().trim().to_owned();
    if name.is_empty() {
        bail!("work item state name cannot be empty");
    }
    Ok(name)
}

fn model_to_view(model: WorkItemStateModel) -> WorkItemStateView {
    WorkItemStateView {
        id: model.id,
        project_id: model.project_id,
        identifier: model.identifier,
        name: model.name,
        position: model.position,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::projects::{CreateProject, create_project};

    #[tokio::test]
    async fn default_work_item_states_are_project_scoped() {
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

        let states = list_work_item_states(&store, "demo")
            .await
            .unwrap()
            .into_iter()
            .map(|state| state.identifier)
            .collect::<Vec<_>>();

        assert_that!(&(states)).is_equal_to(vec!["idea", "open", "in_progress", "done"]);
    }
}
