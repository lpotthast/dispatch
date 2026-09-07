use std::collections::BTreeMap;

use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};

use crate::{
    backend::{
        entities::{
            label_key::{self, LabelKey, LabelKeyActiveModel, LabelKeyModel},
            work_item_label::{self, WorkItemLabel},
        },
        storage::utc_now,
    },
    shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, CLAIMED_FROM_STATE_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY,
        STATE_LABEL_KEY,
    },
};

pub(crate) const BUILT_IN_LABEL_KEYS: [&str; 4] = [
    STATE_LABEL_KEY,
    CLAIMED_FROM_STATE_LABEL_KEY,
    AUTOMATION_BLOCKED_LABEL_KEY,
    FEEDBACK_REQUESTED_LABEL_KEY,
];

pub(crate) async fn ensure_built_in_label_keys_in_conn<C>(conn: &C, project_id: i64) -> Result<()>
where
    C: ConnectionTrait,
{
    for key in BUILT_IN_LABEL_KEYS {
        if let Some(existing) = find(conn, project_id, key).await? {
            if existing.built_in && existing.persistent {
                continue;
            }
            let mut active: LabelKeyActiveModel = existing.into();
            active.built_in = Set(true);
            active.persistent = Set(true);
            active.updated_at = Set(utc_now());
            active
                .update(conn)
                .await
                .context_with(|| format!("failed to mark label key '{key}' as built in"))?;
        } else {
            insert(conn, project_id, key, None, true, true).await?;
        }
    }
    Ok(())
}

pub(crate) async fn ensure_used_key_in_tx<C>(conn: &C, project_id: i64, key: &str) -> Result<()>
where
    C: ConnectionTrait,
{
    if find(conn, project_id, key).await?.is_none() {
        insert(conn, project_id, key, None, false, false).await?;
    }
    Ok(())
}

pub(crate) async fn forget_if_unused_in_tx<C>(conn: &C, project_id: i64, key: &str) -> Result<bool>
where
    C: ConnectionTrait,
{
    let Some(label_key) = find(conn, project_id, key).await? else {
        return Ok(false);
    };
    if label_key.built_in || label_key.persistent {
        return Ok(false);
    }
    let in_use = WorkItemLabel::find()
        .filter(work_item_label::Column::ProjectId.eq(project_id))
        .filter(work_item_label::Column::Key.eq(key))
        .one(conn)
        .await
        .context_with(|| format!("failed to check label key usage for '{key}'"))?
        .is_some();
    if in_use {
        return Ok(false);
    }

    LabelKey::delete_by_id(label_key.id)
        .exec(conn)
        .await
        .context_with(|| format!("failed to forget unused label key '{key}'"))?;
    Ok(true)
}

pub(crate) async fn accent_colors_for_project<C>(
    conn: &C,
    project_id: i64,
) -> Result<BTreeMap<String, String>>
where
    C: ConnectionTrait,
{
    let label_keys = LabelKey::find()
        .filter(label_key::Column::ProjectId.eq(project_id))
        .all(conn)
        .await
        .context("failed to load project label accent colors")?;
    Ok(label_keys
        .into_iter()
        .filter_map(|label_key| {
            label_key
                .accent_color
                .map(|accent_color| (label_key.key, accent_color))
        })
        .collect())
}

async fn find<C>(conn: &C, project_id: i64, key: &str) -> Result<Option<LabelKeyModel>>
where
    C: ConnectionTrait,
{
    Ok(LabelKey::find()
        .filter(label_key::Column::ProjectId.eq(project_id))
        .filter(label_key::Column::Key.eq(key))
        .one(conn)
        .await
        .context_with(|| format!("failed to load label key '{key}'"))?)
}

async fn insert<C>(
    conn: &C,
    project_id: i64,
    key: &str,
    accent_color: Option<String>,
    persistent: bool,
    built_in: bool,
) -> Result<LabelKeyModel>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    Ok(LabelKeyActiveModel {
        project_id: Set(project_id),
        key: Set(key.to_owned()),
        accent_color: Set(accent_color),
        persistent: Set(persistent),
        built_in: Set(built_in),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context_with(|| format!("failed to create label key '{key}'"))?)
}

use super::model::{CreateLabelKey, LabelKeyRecord};
use crate::backend::storage::Transaction;
fn decode(row: LabelKeyModel) -> Result<LabelKeyRecord> {
    Ok(LabelKeyRecord {
        id: row.id,
        project_id: row.project_id,
        key: crate::backend::items::labels::policy::normalize_key(row.key)?,
        accent_color: super::policy::normalize_accent_color(row.accent_color)?,
        persistent: row.persistent,
        built_in: row.built_in,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
pub(crate) fn encode(record: LabelKeyRecord) -> LabelKeyModel {
    LabelKeyModel {
        id: record.id,
        project_id: record.project_id,
        key: record.key,
        accent_color: record.accent_color,
        persistent: record.persistent,
        built_in: record.built_in,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}
pub(crate) struct CatalogRepository;
impl CatalogRepository {
    pub(crate) async fn accent_colors_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<BTreeMap<String, String>> {
        accent_colors_for_project(transaction.connection(), project_id).await
    }
    pub(crate) async fn exists_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        key: &str,
    ) -> Result<bool> {
        Ok(find(transaction.connection(), project_id, key)
            .await?
            .is_some())
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<LabelKeyRecord> {
        let row = LabelKey::find_by_id(id)
            .filter(label_key::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load label key")?
            .ok_or_else(|| report!("label key {id} does not exist in this project"))?;
        decode(row)
    }
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        input: CreateLabelKey,
    ) -> Result<LabelKeyRecord> {
        decode(
            insert(
                transaction.connection(),
                project_id,
                &input.key,
                input.accent_color,
                input.persistent,
                false,
            )
            .await?,
        )
    }
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        record: LabelKeyRecord,
    ) -> Result<LabelKeyRecord> {
        use sea_orm::IntoActiveModel;
        decode(
            encode(record)
                .into_active_model()
                .reset_all()
                .update(transaction.connection())
                .await
                .context("failed to update label key")?,
        )
    }
    pub(crate) async fn forget_if_unused_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        key: &str,
    ) -> Result<bool> {
        forget_if_unused_in_tx(transaction.connection(), project_id, key).await
    }
}
#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use tempfile::TempDir;

    use super::super::policy::normalize_accent_color;
    use super::*;
    use crate::backend::projects::repository::ProjectRepository;
    use crate::{
        backend::{
            entities::label_key::LabelKeyActiveModel, items, projects::CreateProject,
            storage::Store,
        },
        shared::view_models::CreateWorkItemLabelRequest,
    };

    async fn test_store() -> (TempDir, Store, i64) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
        let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
        (temp, store, project_id)
    }

    async fn create_labeled_item(store: &Store, title: &str, key: &str) -> i64 {
        let event_bus = crate::backend::events::UiEventBus::new();

        crate::backend::items::creation::tests::service(store, event_bus.clone())
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
                items::CreateWorkItem {
                    title: title.to_owned(),
                    description: format!("Description for {title}"),
                    state: "open".to_owned(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: vec![CreateWorkItemLabelRequest {
                        key: key.to_owned(),
                        value: Some("value".to_owned()),
                    }],
                },
                Default::default(),
            )
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn projects_seed_core_built_ins_and_used_keys_follow_item_lifecycle() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store, project_id) = test_store().await;
        let built_ins = LabelKey::find()
            .filter(label_key::Column::ProjectId.eq(project_id))
            .all(store.db().as_ref())
            .await
            .unwrap();
        let mut actual_built_ins = built_ins
            .iter()
            .map(|label_key| label_key.key.as_str())
            .collect::<Vec<_>>();
        actual_built_ins.sort_unstable();
        let mut expected_built_ins = BUILT_IN_LABEL_KEYS;
        expected_built_ins.sort_unstable();
        assert_that!(&(actual_built_ins)).is_equal_to(expected_built_ins);
        assert_that!(
            &(built_ins
                .iter()
                .all(|label_key| label_key.built_in && label_key.persistent))
        )
        .is_true();

        let first = create_labeled_item(&store, "First", "severity").await;
        let second = create_labeled_item(&store, "Second", "severity").await;
        let discovered = find(store.db().as_ref(), project_id, "severity")
            .await
            .unwrap()
            .unwrap();
        assert_that!(&(discovered.persistent)).is_false();
        assert_that!(&(discovered.built_in)).is_false();

        crate::backend::items::tests::service(&store, event_bus.clone())
            .delete(
                crate::backend::projects::ProjectReference::Name("demo"),
                first,
            )
            .await
            .unwrap();
        assert_that!(
            &(find(store.db().as_ref(), project_id, "severity")
                .await
                .unwrap()
                .is_some())
        )
        .is_true();

        crate::backend::items::tests::service(&store, event_bus.clone())
            .delete(
                crate::backend::projects::ProjectReference::Name("demo"),
                second,
            )
            .await
            .unwrap();
        assert_that!(
            &(find(store.db().as_ref(), project_id, "severity")
                .await
                .unwrap()
                .is_none())
        )
        .is_true();
        assert_that!(
            &(find(store.db().as_ref(), project_id, STATE_LABEL_KEY)
                .await
                .unwrap()
                .is_some())
        )
        .is_true();
    }

    #[tokio::test]
    async fn persistent_configuration_survives_zero_usage_until_unpersisted() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store, project_id) = test_store().await;
        let item_id = create_labeled_item(&store, "Persistent", "area").await;
        let existing = find(store.db().as_ref(), project_id, "area")
            .await
            .unwrap()
            .unwrap();
        let mut active: LabelKeyActiveModel = existing.into();
        active.persistent = Set(true);
        active.accent_color = Set(Some("#aabbcc".to_owned()));
        active.update(store.db().as_ref()).await.unwrap();

        crate::backend::items::tests::service(&store, event_bus.clone())
            .delete(
                crate::backend::projects::ProjectReference::Name("demo"),
                item_id,
            )
            .await
            .unwrap();
        let persisted = find(store.db().as_ref(), project_id, "area")
            .await
            .unwrap()
            .unwrap();
        assert_that!(&(persisted.accent_color)).is_equal_to(Some("#aabbcc".to_owned()));

        let mut active: LabelKeyActiveModel = persisted.into();
        active.persistent = Set(false);
        active.update(store.db().as_ref()).await.unwrap();
        assert_that!(
            &(forget_if_unused_in_tx(store.db().as_ref(), project_id, "area")
                .await
                .unwrap())
        )
        .is_true();
        assert_that!(
            &(find(store.db().as_ref(), project_id, "area")
                .await
                .unwrap()
                .is_none())
        )
        .is_true();
    }

    #[tokio::test]
    async fn renaming_the_final_label_forgets_the_old_key_and_discovers_the_new_key() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store, project_id) = test_store().await;
        let item_id = create_labeled_item(&store, "Rename", "area").await;
        let item = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", item_id)
        .await
        .unwrap();
        let label = item
            .labels
            .iter()
            .find(|label| label.key == "area")
            .unwrap();

        crate::backend::items::labels::tests::service(&store, event_bus.clone())
            .update(
                "demo",
                item_id,
                label.id,
                dispatch_types::UpdateWorkItemLabelRequest {
                    key: Some("component".to_owned()),
                    value: None,
                    expect_version: None,
                },
                Default::default(),
            )
            .await
            .unwrap();

        assert_that!(
            &(find(store.db().as_ref(), project_id, "area")
                .await
                .unwrap()
                .is_none())
        )
        .is_true();
        assert_that!(
            &(find(store.db().as_ref(), project_id, "component")
                .await
                .unwrap()
                .is_some())
        )
        .is_true();
    }

    #[test]
    fn accent_colors_are_optional_canonical_hex_values() {
        assert_that!(&(normalize_accent_color(None).unwrap())).is_equal_to(None);
        assert_that!(&(normalize_accent_color(Some("  ".to_owned())).unwrap())).is_equal_to(None);
        assert_that!(&(normalize_accent_color(Some("#A1B2C3".to_owned())).unwrap()))
            .is_equal_to(Some("#a1b2c3".to_owned()));
        assert_that!(
            &(normalize_accent_color(Some("red".to_owned()))
                .unwrap_err()
                .to_string())
        )
        .contains("accent color must use #RRGGBB");
    }
}
