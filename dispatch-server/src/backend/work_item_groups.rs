use std::collections::{BTreeMap, BTreeSet};

use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};

use crate::{
    backend::{
        entities::{
            work_item::{self, WorkItem, WorkItemActiveModel},
            work_item_group::{self, WorkItemGroup, WorkItemGroupActiveModel},
        },
        events, projects,
        request_attribution::RequestAttribution,
        storage::{Store, utc_now},
        work_item_events, work_items,
    },
    shared::view_models::{
        CreateWorkItemGroupRequest, WorkItemEventType, WorkItemGroupSummaryView, WorkItemGroupView,
    },
};

pub(crate) fn normalize_group_key(value: String) -> Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        bail!("work-group key must use lowercase letters, digits, '.', '_' or '-'");
    }
    Ok(value)
}

fn normalize_group_name(value: String) -> Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        bail!("work-group name cannot be empty");
    }
    if value.len() > 200 {
        bail!("work-group name cannot exceed 200 bytes");
    }
    Ok(value)
}

pub(crate) async fn list_groups(
    store: &Store,
    project_name: &str,
) -> Result<Vec<WorkItemGroupView>> {
    let project_id = projects::project_id(store, project_name).await?;
    let groups = WorkItemGroup::find()
        .filter(work_item_group::Column::ProjectId.eq(project_id))
        .order_by_asc(work_item_group::Column::Name)
        .order_by_asc(work_item_group::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to list work groups")?;
    let counts = WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .all(store.db().as_ref())
        .await
        .context("failed to count grouped work items")?
        .into_iter()
        .filter_map(|item| item.work_group_id)
        .fold(BTreeMap::<i64, u64>::new(), |mut counts, group_id| {
            *counts.entry(group_id).or_default() += 1;
            counts
        });
    Ok(groups
        .into_iter()
        .map(|group| {
            let count = counts.get(&group.id).copied().unwrap_or(0);
            group_view(group, count)
        })
        .collect())
}

pub(crate) async fn create_group(
    store: &Store,
    project_name: &str,
    request: CreateWorkItemGroupRequest,
    attribution: &RequestAttribution,
) -> Result<WorkItemGroupView> {
    let project_id = projects::project_id(store, project_name).await?;
    let key = normalize_group_key(request.key)?;
    let name = normalize_group_name(request.name)?;
    if let Some(existing) = WorkItemGroup::find()
        .filter(work_item_group::Column::ProjectId.eq(project_id))
        .filter(work_item_group::Column::GroupKey.eq(&key))
        .one(store.db().as_ref())
        .await
        .context("failed to look up work group")?
    {
        if existing.name != name {
            bail!(
                "work group '{key}' already exists as '{}'; use that name or choose another key",
                existing.name
            );
        }
        let count = WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::WorkGroupId.eq(existing.id))
            .count(store.db().as_ref())
            .await
            .context("failed to count work-group items")?;
        return Ok(group_view(existing, count));
    }

    let now = utc_now();
    let event = attribution.event();
    let group = WorkItemGroupActiveModel {
        project_id: Set(project_id),
        group_key: Set(key),
        name: Set(name),
        actor_id: Set(event.actor_id.map(ToOwned::to_owned)),
        agent_run_id: Set(event.agent_run_id),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .context("failed to create work group")?;
    Ok(group_view(group, 0))
}

pub(crate) async fn assign_items(
    store: &Store,
    project_name: &str,
    group_key: &str,
    item_ids: Vec<i64>,
    attribution: &RequestAttribution,
) -> Result<WorkItemGroupView> {
    let project_id = projects::project_id(store, project_name).await?;
    let group_key = normalize_group_key(group_key.to_owned())?;
    let item_ids = item_ids.into_iter().collect::<BTreeSet<_>>();
    if item_ids.is_empty() {
        bail!("work-group assignment requires at least one item id");
    }
    if item_ids.iter().any(|item_id| *item_id <= 0) {
        bail!("work-group item ids must be positive");
    }
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start work-group assignment")?;
    let group = WorkItemGroup::find()
        .filter(work_item_group::Column::ProjectId.eq(project_id))
        .filter(work_item_group::Column::GroupKey.eq(&group_key))
        .one(&txn)
        .await
        .context("failed to load work group")?
        .ok_or_else(|| report!("work group '{group_key}' does not exist in this project"))?;
    let now = utc_now();
    let mut changed_ids = Vec::new();
    for item_id in &item_ids {
        let item = work_items::get(&txn, project_id, *item_id).await?;
        if item.work_group_id == Some(group.id) {
            continue;
        }
        if let Some(existing_group_id) = item.work_group_id {
            bail!(
                "item {item_id} already belongs to work group {existing_group_id}; remove it before assigning another group"
            );
        }
        let version = item.version;
        let mut active: WorkItemActiveModel = item.into();
        active.work_group_id = Set(Some(group.id));
        active.version = Set(version + 1);
        active.updated_at = Set(now.clone());
        active
            .update(&txn)
            .await
            .context_with(|| format!("failed to assign item {item_id} to work group"))?;
        work_item_events::record_event_with_attribution_in_tx(
            &txn,
            project_id,
            Some(*item_id),
            WorkItemEventType::ItemUpdated,
            &format!("Assigned to work group {group_key}"),
            attribution.event(),
        )
        .await?;
        changed_ids.push(*item_id);
    }
    let item_count = WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .filter(work_item::Column::WorkGroupId.eq(group.id))
        .count(&txn)
        .await
        .context("failed to count assigned work-group items")?;
    txn.commit()
        .await
        .context("failed to commit work-group assignment")?;
    for item_id in changed_ids {
        events::publish_work_item_changed(project_name, item_id);
    }
    Ok(group_view(group, item_count))
}

pub(crate) async fn summaries_for_items(
    store: &Store,
    project_id: i64,
    group_ids: impl IntoIterator<Item = i64>,
) -> Result<BTreeMap<i64, WorkItemGroupSummaryView>> {
    let group_ids = group_ids.into_iter().collect::<BTreeSet<_>>();
    if group_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(WorkItemGroup::find()
        .filter(work_item_group::Column::ProjectId.eq(project_id))
        .filter(work_item_group::Column::Id.is_in(group_ids))
        .all(store.db().as_ref())
        .await
        .context("failed to load work groups for items")?
        .into_iter()
        .map(|group| {
            (
                group.id,
                WorkItemGroupSummaryView {
                    id: group.id,
                    key: group.group_key,
                    name: group.name,
                },
            )
        })
        .collect())
}

fn group_view(group: work_item_group::Model, item_count: u64) -> WorkItemGroupView {
    WorkItemGroupView {
        id: group.id,
        project_id: group.project_id,
        key: group.group_key,
        name: group.name,
        item_count,
        actor_id: group.actor_id,
        agent_run_id: group.agent_run_id,
        created_at: group.created_at,
        updated_at: group.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use dispatch_types::WorkItemView;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        items::{CreateWorkItem, create_item},
        projects::{CreateProject, create_project},
    };

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        for name in ["demo", "other"] {
            create_project(
                &store,
                CreateProject {
                    name: name.to_owned(),
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
        }
        (temp, store)
    }

    async fn create_test_item(store: &Store, project: &str, title: &str) -> WorkItemView {
        create_item(
            store,
            project,
            CreateWorkItem {
                title: title.to_owned(),
                description: format!("Test item {title}"),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn groups_are_idempotent_and_assignment_updates_item_views() {
        let (_temp, store) = test_store().await;
        let attribution = RequestAttribution::default();
        let created = create_group(
            &store,
            "demo",
            CreateWorkItemGroupRequest {
                key: "review-42".to_owned(),
                name: "Review 42".to_owned(),
            },
            &attribution,
        )
        .await
        .unwrap();
        let repeated = create_group(
            &store,
            "demo",
            CreateWorkItemGroupRequest {
                key: "review-42".to_owned(),
                name: "Review 42".to_owned(),
            },
            &attribution,
        )
        .await
        .unwrap();
        assert_eq!(created.id, repeated.id);

        let first = create_test_item(&store, "demo", "First").await;
        let second = create_test_item(&store, "demo", "Second").await;
        let assigned = assign_items(
            &store,
            "demo",
            "review-42",
            vec![first.id, second.id, first.id],
            &attribution,
        )
        .await
        .unwrap();
        assert_eq!(assigned.item_count, 2);
        let first = crate::backend::items::get_item(&store, "demo", first.id)
            .await
            .unwrap();
        assert_eq!(first.work_group.unwrap().key, "review-42");
        assert_eq!(first.version, 2);
        assert_eq!(list_groups(&store, "demo").await.unwrap()[0].item_count, 2);
    }

    #[tokio::test]
    async fn assignment_is_project_scoped_and_atomic_on_conflict() {
        let (_temp, store) = test_store().await;
        let attribution = RequestAttribution::default();
        for key in ["first", "second"] {
            create_group(
                &store,
                "demo",
                CreateWorkItemGroupRequest {
                    key: key.to_owned(),
                    name: key.to_owned(),
                },
                &attribution,
            )
            .await
            .unwrap();
        }
        let available = create_test_item(&store, "demo", "Available").await;
        let occupied = create_test_item(&store, "demo", "Occupied").await;
        assign_items(&store, "demo", "second", vec![occupied.id], &attribution)
            .await
            .unwrap();

        let error = assign_items(
            &store,
            "demo",
            "first",
            vec![available.id, occupied.id],
            &attribution,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("already belongs"));
        let available = crate::backend::items::get_item(&store, "demo", available.id)
            .await
            .unwrap();
        assert!(available.work_group.is_none());
        assert_eq!(available.version, 1);

        let other = create_test_item(&store, "other", "Other").await;
        let error = assign_items(&store, "demo", "first", vec![other.id], &attribution)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("does not exist in this project"));
    }
}
