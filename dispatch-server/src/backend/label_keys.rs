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

pub(crate) async fn ensure_used_key_in_tx<C>(
    conn: &C,
    project_id: i64,
    key: &str,
) -> Result<LabelKeyModel>
where
    C: ConnectionTrait,
{
    if let Some(existing) = find(conn, project_id, key).await? {
        return Ok(existing);
    }
    insert(conn, project_id, key, None, false, false).await
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

pub(crate) fn normalize_accent_color(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        bail!("accent color must use #RRGGBB hexadecimal notation");
    }
    Ok(Some(value.to_ascii_lowercase()))
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

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use tempfile::TempDir;

    use super::*;
    use crate::{
        backend::{
            entities::label_key::LabelKeyActiveModel,
            item_label_service, items,
            projects::{CreateProject, create_project, project_id},
            storage::Store,
        },
        shared::view_models::CreateWorkItemLabelRequest,
    };

    async fn test_store() -> (TempDir, Store, i64) {
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
        let project_id = project_id(&store, "demo").await.unwrap();
        (temp, store, project_id)
    }

    async fn create_labeled_item(store: &Store, title: &str, key: &str) -> i64 {
        items::create_item(
            store,
            "demo",
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
        )
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn projects_seed_core_built_ins_and_used_keys_follow_item_lifecycle() {
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

        items::delete_item(&store, "demo", first).await.unwrap();
        assert_that!(
            &(find(store.db().as_ref(), project_id, "severity")
                .await
                .unwrap()
                .is_some())
        )
        .is_true();

        items::delete_item(&store, "demo", second).await.unwrap();
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

        items::delete_item(&store, "demo", item_id).await.unwrap();
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
        let (_temp, store, project_id) = test_store().await;
        let item_id = create_labeled_item(&store, "Rename", "area").await;
        let item = items::get_item(&store, "demo", item_id).await.unwrap();
        let label = item
            .labels
            .iter()
            .find(|label| label.key == "area")
            .unwrap();

        item_label_service::update_label(
            &store,
            "demo",
            item_id,
            label.id,
            Some("component".to_owned()),
            None,
            None,
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
