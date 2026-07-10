use std::collections::BTreeMap;

use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, Statement,
};

use crate::{
    backend::{
        entities::work_item_label::{
            self, WorkItemLabel, WorkItemLabelActiveModel, WorkItemLabelModel,
        },
        item_labels,
        storage::utc_now,
    },
    shared::view_models::{ProjectLabelView, STATE_LABEL_KEY, WorkItemLabelView},
};

pub(crate) async fn item_ids_with_state<C>(
    conn: &C,
    project_id: i64,
    state: &str,
) -> Result<Vec<i64>>
where
    C: sea_orm::ConnectionTrait,
{
    let labels = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::Key.eq(STATE_LABEL_KEY))
        .filter(work_item_label::Column::Value.eq(state))
        .all(conn)
        .await
        .context_with(|| format!("failed to list items with state label '{state}'"))?;
    Ok(labels.into_iter().map(|label| label.work_item_id).collect())
}

pub(crate) async fn insert_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    key: &str,
    value: Option<&str>,
) -> Result<WorkItemLabelModel>
where
    C: sea_orm::ConnectionTrait,
{
    let now = utc_now();
    let active = WorkItemLabelActiveModel {
        project_id: Set(project_id),
        work_item_id: Set(item_id),
        key: Set(key.to_owned()),
        value: Set(value.map(ToOwned::to_owned)),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    };
    Ok(active.insert(conn).await.context_with(|| {
        format!(
            "failed to add label '{}'",
            item_labels::format_label(key, value)
        )
    })?)
}

pub(crate) async fn upsert_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    key: &str,
    value: Option<&str>,
) -> Result<WorkItemLabelModel>
where
    C: sea_orm::ConnectionTrait,
{
    if let Some(existing) = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.eq(item_id))
        .filter(work_item_label::Column::Key.eq(key))
        .one(conn)
        .await
        .context_with(|| format!("failed to load label '{key}'"))?
    {
        let mut active: WorkItemLabelActiveModel = existing.into();
        active.value = Set(value.map(ToOwned::to_owned));
        active.updated_at = Set(utc_now());
        Ok(active
            .update(conn)
            .await
            .context_with(|| format!("failed to update label '{key}'"))?)
    } else {
        insert_in_tx(conn, project_id, item_id, key, value).await
    }
}

pub(crate) async fn delete_by_key_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    key: &str,
) -> Result<()>
where
    C: sea_orm::ConnectionTrait,
{
    WorkItemLabel::delete_many()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.eq(item_id))
        .filter(work_item_label::Column::Key.eq(key))
        .exec(conn)
        .await
        .context_with(|| format!("failed to delete label '{key}'"))?;
    Ok(())
}

pub(crate) async fn delete_by_id_in_tx<C>(conn: &C, label_id: i64) -> Result<()>
where
    C: ConnectionTrait,
{
    WorkItemLabel::delete_by_id(label_id)
        .exec(conn)
        .await
        .context_with(|| format!("failed to delete label {label_id}"))?;
    Ok(())
}

pub(crate) async fn get_for_item<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    label_id: i64,
) -> Result<WorkItemLabelModel>
where
    C: ConnectionTrait,
{
    WorkItemLabel::find_by_id(label_id)
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.eq(item_id))
        .one(conn)
        .await
        .context_with(|| format!("failed to load label {label_id}"))?
        .ok_or_else(|| report!("label {label_id} does not exist on item {item_id}"))
}

pub(crate) async fn item_has_key<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    key: &str,
    except_label_id: Option<i64>,
) -> Result<bool>
where
    C: ConnectionTrait,
{
    let mut query = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.eq(item_id))
        .filter(work_item_label::Column::Key.eq(key));
    if let Some(label_id) = except_label_id {
        query = query.filter(work_item_label::Column::Id.ne(label_id));
    }

    Ok(query
        .one(conn)
        .await
        .context_with(|| format!("failed to check existing label '{key}'"))?
        .is_some())
}

pub(crate) async fn update_in_tx<C>(
    conn: &C,
    label: WorkItemLabelModel,
    key: String,
    value: Option<String>,
) -> Result<WorkItemLabelModel>
where
    C: ConnectionTrait,
{
    let mut active: WorkItemLabelActiveModel = label.into();
    active.key = Set(key.clone());
    active.value = Set(value.clone());
    active.updated_at = Set(utc_now());
    Ok(active
        .update(conn)
        .await
        .context_with(|| format!("failed to update label '{key}'"))?)
}

pub(crate) async fn project_label_summaries<C>(
    conn: &C,
    project_id: i64,
) -> Result<Vec<ProjectLabelView>>
where
    C: ConnectionTrait,
{
    let rows = conn
        .query_all(Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            r#"
            SELECT label_key,
                   label_value,
                   COUNT(*) AS usage_count,
                   MAX(updated_at) AS last_used_at
            FROM work_item_labels
            WHERE project_id = ?1
            GROUP BY label_key, label_value
            ORDER BY label_key ASC, label_value ASC
            "#,
            vec![project_id.into()],
        ))
        .await
        .context("failed to list project labels")?;

    rows.into_iter()
        .map(|row| {
            Ok(ProjectLabelView {
                key: row
                    .try_get("", "label_key")
                    .context("failed to read project label key")?,
                value: row
                    .try_get("", "label_value")
                    .context("failed to read project label value")?,
                usage_count: row
                    .try_get("", "usage_count")
                    .context("failed to read project label usage count")?,
                last_used_at: row
                    .try_get("", "last_used_at")
                    .context("failed to read project label last-used timestamp")?,
            })
        })
        .collect()
}

pub(crate) async fn for_item<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
) -> Result<Vec<WorkItemLabelView>>
where
    C: sea_orm::ConnectionTrait,
{
    let labels = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.eq(item_id))
        .order_by_asc(work_item_label::Column::Key)
        .order_by_asc(work_item_label::Column::Value)
        .all(conn)
        .await
        .context("failed to list item labels")?;
    Ok(labels.into_iter().map(to_view).collect())
}

pub(crate) async fn for_items<C>(
    conn: &C,
    project_id: i64,
    item_ids: &[i64],
) -> Result<BTreeMap<i64, Vec<WorkItemLabelView>>>
where
    C: sea_orm::ConnectionTrait,
{
    let labels = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::WorkItemId.is_in(item_ids.iter().copied()))
        .order_by_asc(work_item_label::Column::WorkItemId)
        .order_by_asc(work_item_label::Column::Key)
        .order_by_asc(work_item_label::Column::Value)
        .all(conn)
        .await
        .context("failed to list item labels")?;

    let mut labels_by_item = BTreeMap::<i64, Vec<WorkItemLabelView>>::new();
    for label in labels {
        labels_by_item
            .entry(label.work_item_id)
            .or_default()
            .push(to_view(label));
    }
    Ok(labels_by_item)
}

pub(crate) fn to_view(label: WorkItemLabelModel) -> WorkItemLabelView {
    WorkItemLabelView {
        id: label.id,
        project_id: label.project_id,
        work_item_id: label.work_item_id,
        key: label.key,
        value: label.value,
        created_at: label.created_at,
        updated_at: label.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        entities::work_item::WorkItemActiveModel,
        projects::{CreateProject, create_project, project_id},
        storage::Store,
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

    async fn create_test_item(store: &Store, project_id: i64, title: &str) -> i64 {
        WorkItemActiveModel {
            project_id: Set(project_id),
            title: Set(title.to_owned()),
            description: Set(format!("Description for {title}")),
            claimed_by: Set(None),
            claimed_at: Set(None),
            claim_expires_at: Set(None),
            finished_at: Set(None),
            agent_model_override: Set(None),
            agent_reasoning_effort_override: Set(None),
            version: Set(1),
            created_at: Set("2026-06-18T00:00:00Z".to_owned()),
            updated_at: Set("2026-06-18T00:00:00Z".to_owned()),
            ..Default::default()
        }
        .insert(store.db().as_ref())
        .await
        .unwrap()
        .id
    }

    async fn insert_label_with_timestamp(
        store: &Store,
        project_id: i64,
        item_id: i64,
        key: &str,
        value: Option<&str>,
        timestamp: &str,
    ) -> WorkItemLabelModel {
        WorkItemLabelActiveModel {
            project_id: Set(project_id),
            work_item_id: Set(item_id),
            key: Set(key.to_owned()),
            value: Set(value.map(ToOwned::to_owned)),
            created_at: Set(timestamp.to_owned()),
            updated_at: Set(timestamp.to_owned()),
            ..Default::default()
        }
        .insert(store.db().as_ref())
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn project_label_summaries_group_values_and_scope_to_project() {
        let (_temp, store) = test_store().await;
        let demo_project_id = project_id(&store, "demo").await.unwrap();
        let other_project_id = project_id(&store, "other").await.unwrap();
        let first = create_test_item(&store, demo_project_id, "First").await;
        let second = create_test_item(&store, demo_project_id, "Second").await;
        let other = create_test_item(&store, other_project_id, "Other").await;

        insert_label_with_timestamp(
            &store,
            demo_project_id,
            first,
            "severity",
            Some("high"),
            "2026-06-18T00:00:01Z",
        )
        .await;
        insert_label_with_timestamp(
            &store,
            demo_project_id,
            second,
            "severity",
            Some("high"),
            "2026-06-18T00:00:03Z",
        )
        .await;
        insert_label_with_timestamp(
            &store,
            demo_project_id,
            second,
            "bug",
            None,
            "2026-06-18T00:00:02Z",
        )
        .await;
        insert_label_with_timestamp(
            &store,
            other_project_id,
            other,
            "severity",
            Some("high"),
            "2026-06-18T00:00:04Z",
        )
        .await;

        let summaries = project_label_summaries(store.db().as_ref(), demo_project_id)
            .await
            .unwrap();
        let severity = summaries
            .iter()
            .find(|label| label.key == "severity" && label.value.as_deref() == Some("high"))
            .unwrap();
        let bug = summaries
            .iter()
            .find(|label| label.key == "bug" && label.value.is_none())
            .unwrap();

        assert_eq!(severity.usage_count, 2);
        assert_eq!(severity.last_used_at, "2026-06-18T00:00:03Z");
        assert_eq!(bug.usage_count, 1);
    }

    #[tokio::test]
    async fn item_has_key_can_ignore_the_current_label() {
        let (_temp, store) = test_store().await;
        let demo_project_id = project_id(&store, "demo").await.unwrap();
        let item_id = create_test_item(&store, demo_project_id, "Labeled").await;
        let label = insert_label_with_timestamp(
            &store,
            demo_project_id,
            item_id,
            "severity",
            Some("high"),
            "2026-06-18T00:00:01Z",
        )
        .await;

        assert!(
            item_has_key(
                store.db().as_ref(),
                demo_project_id,
                item_id,
                "severity",
                None
            )
            .await
            .unwrap()
        );
        assert!(
            !item_has_key(
                store.db().as_ref(),
                demo_project_id,
                item_id,
                "severity",
                Some(label.id),
            )
            .await
            .unwrap()
        );
    }
}
