use rootcause::{Result, prelude::*};
use sea_orm::{DatabaseTransaction, TransactionTrait};

use crate::{
    backend::{
        entities::work_item::WorkItemModel, events, item_label_mutations, projects, storage::Store,
        work_item_events, work_item_labels, work_item_views, work_items,
    },
    shared::view_models::{
        DeleteWorkItemLabelResponse, ProjectLabelView, WorkItemLabelView, WorkItemView,
    },
};

pub async fn list_item_labels(
    store: &Store,
    project_name: &str,
    item_id: i64,
) -> Result<Vec<WorkItemLabelView>> {
    let project_id = projects::project_id(store, project_name).await?;
    work_items::get(store.db().as_ref(), project_id, item_id).await?;
    work_item_labels::for_item(store.db().as_ref(), project_id, item_id).await
}

pub async fn list_project_labels(
    store: &Store,
    project_name: &str,
) -> Result<Vec<ProjectLabelView>> {
    let project_id = projects::project_id(store, project_name).await?;
    work_item_labels::project_label_summaries(store.db().as_ref(), project_id).await
}

pub async fn add_label(
    store: &Store,
    project_name: &str,
    item_id: i64,
    key: String,
    value: Option<String>,
    expect_version: Option<i64>,
) -> Result<WorkItemView> {
    add_label_with_attribution(
        store,
        project_name,
        item_id,
        key,
        value,
        expect_version,
        work_item_events::EventAttribution::default(),
    )
    .await
}

pub(crate) async fn add_label_with_attribution(
    store: &Store,
    project_name: &str,
    item_id: i64,
    key: String,
    value: Option<String>,
    expect_version: Option<i64>,
    attribution: work_item_events::EventAttribution<'_>,
) -> Result<WorkItemView> {
    let label = item_label_mutations::AddLabelMutation::new(key, value)?;
    let context =
        LabelMutationContext::start(store, project_name, item_id, expect_version, "label add")
            .await?;
    if work_item_labels::item_has_key(&context.txn, context.project_id, item_id, &label.key, None)
        .await?
    {
        bail!("item already has label '{}'", label.key);
    }

    work_item_labels::insert_in_tx(
        &context.txn,
        context.project_id,
        item_id,
        &label.key,
        label.value.as_deref(),
    )
    .await?;
    context
        .finish(
            store,
            project_name,
            label.added_event(),
            "label add",
            attribution,
        )
        .await
}

pub async fn update_label(
    store: &Store,
    project_name: &str,
    item_id: i64,
    label_id: i64,
    key: Option<String>,
    value: Option<Option<String>>,
    expect_version: Option<i64>,
) -> Result<WorkItemView> {
    update_label_with_attribution(
        store,
        project_name,
        item_id,
        UpdateLabelInput {
            label_id,
            key,
            value,
            expect_version,
        },
        work_item_events::EventAttribution::default(),
    )
    .await
}

pub(crate) struct UpdateLabelInput {
    pub(crate) label_id: i64,
    pub(crate) key: Option<String>,
    pub(crate) value: Option<Option<String>>,
    pub(crate) expect_version: Option<i64>,
}

pub(crate) async fn update_label_with_attribution(
    store: &Store,
    project_name: &str,
    item_id: i64,
    input: UpdateLabelInput,
    attribution: work_item_events::EventAttribution<'_>,
) -> Result<WorkItemView> {
    let label = item_label_mutations::UpdateLabelMutation::new(input.key, input.value)?;

    let context = LabelMutationContext::start(
        store,
        project_name,
        item_id,
        input.expect_version,
        "label update",
    )
    .await?;
    let existing =
        work_item_labels::get_for_item(&context.txn, context.project_id, item_id, input.label_id)
            .await?;
    let label = label.apply_to(&existing)?;
    if work_item_labels::item_has_key(
        &context.txn,
        context.project_id,
        item_id,
        &label.key,
        Some(input.label_id),
    )
    .await?
    {
        bail!("item already has label '{}'", label.key);
    }

    let event = label.updated_event();
    work_item_labels::update_in_tx(
        &context.txn,
        existing,
        label.key.clone(),
        label.value.clone(),
    )
    .await?;
    context
        .finish(store, project_name, event, "label update", attribution)
        .await
}

pub async fn delete_label(
    store: &Store,
    project_name: &str,
    item_id: i64,
    label_id: i64,
    expect_version: Option<i64>,
) -> Result<DeleteWorkItemLabelResponse> {
    delete_label_with_attribution(
        store,
        project_name,
        item_id,
        label_id,
        expect_version,
        work_item_events::EventAttribution::default(),
    )
    .await
}

pub(crate) async fn delete_label_with_attribution(
    store: &Store,
    project_name: &str,
    item_id: i64,
    label_id: i64,
    expect_version: Option<i64>,
    attribution: work_item_events::EventAttribution<'_>,
) -> Result<DeleteWorkItemLabelResponse> {
    let context =
        LabelMutationContext::start(store, project_name, item_id, expect_version, "label delete")
            .await?;
    let label =
        work_item_labels::get_for_item(&context.txn, context.project_id, item_id, label_id).await?;
    let deletion = item_label_mutations::DeleteLabelMutation::new(&label)?;
    let event = deletion.deleted_event();

    work_item_labels::delete_by_id_in_tx(&context.txn, deletion.label_id()).await?;
    let work_item = context
        .finish(store, project_name, event, "label delete", attribution)
        .await?;
    Ok(DeleteWorkItemLabelResponse {
        deleted: true,
        label_id: deletion.label_id(),
        work_item,
    })
}

struct LabelMutationContext {
    project_id: i64,
    item: WorkItemModel,
    txn: DatabaseTransaction,
}

impl LabelMutationContext {
    async fn start(
        store: &Store,
        project_name: &str,
        item_id: i64,
        expect_version: Option<i64>,
        operation: &'static str,
    ) -> Result<Self> {
        let project_id = projects::project_id(store, project_name).await?;
        let txn = store
            .db()
            .begin()
            .await
            .context_with(|| format!("failed to start {operation}"))?;
        let item = work_items::get(&txn, project_id, item_id).await?;
        work_items::check_expected_version(expect_version, item.version)?;

        Ok(Self {
            project_id,
            item,
            txn,
        })
    }

    async fn finish(
        self,
        store: &Store,
        project_name: &str,
        event: item_label_mutations::LabelMutationEvent,
        operation: &'static str,
        attribution: work_item_events::EventAttribution<'_>,
    ) -> Result<WorkItemView> {
        let Self {
            project_id,
            item,
            txn,
        } = self;
        let item_id = item.id;
        let updated = work_items::touch(&txn, item).await?;
        work_item_events::record_event_with_attribution_in_tx(
            &txn,
            project_id,
            Some(item_id),
            event.event_type,
            &event.body,
            attribution,
        )
        .await?;
        txn.commit()
            .await
            .context_with(|| format!("failed to commit {operation}"))?;
        events::publish_work_item_changed(project_name, item_id);
        work_item_views::model_to_view(store, updated).await
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        items::{CreateWorkItem, create_item, list_events},
        projects::{CreateProject, create_project},
    };
    use crate::shared::view_models::{STATE_LABEL_KEY, WorkItemEventType};

    async fn test_store() -> (TempDir, Store) {
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
        (temp, store)
    }

    #[tokio::test]
    async fn add_update_and_delete_label_touch_item_and_preserve_state_label() {
        let (_temp, store) = test_store().await;
        let item = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: "Label item".to_owned(),
                description: "Exercise label service behavior".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();

        let added = add_label(
            &store,
            "demo",
            item.id,
            "priority".to_owned(),
            Some("high".to_owned()),
            Some(item.version),
        )
        .await
        .unwrap();
        let label_id = added
            .labels
            .iter()
            .find(|label| label.key == "priority")
            .unwrap()
            .id;

        let updated = update_label(
            &store,
            "demo",
            item.id,
            label_id,
            None,
            Some(Some("low".to_owned())),
            Some(added.version),
        )
        .await
        .unwrap();
        let deleted = delete_label(&store, "demo", item.id, label_id, Some(updated.version))
            .await
            .unwrap();

        assert_that!(&(added.version)).is_equal_to(item.version + 1);
        assert_that!(&(updated.version)).is_equal_to(added.version + 1);
        assert_that!(&(deleted.work_item.version)).is_equal_to(updated.version + 1);
        assert_that!(&(deleted.deleted)).is_true();
        assert_that!(&(deleted.label_id)).is_equal_to(label_id);
        assert_that!(&(deleted.work_item.state.as_deref())).is_equal_to(Some("open"));
        assert_that!(
            &(!deleted
                .work_item
                .labels
                .iter()
                .any(|label| label.key == "priority"))
        )
        .is_true();

        let events = list_events(&store, "demo", Some(item.id), None)
            .await
            .unwrap();
        let label_events: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event.event_type,
                    WorkItemEventType::LabelAdded
                        | WorkItemEventType::LabelUpdated
                        | WorkItemEventType::LabelDeleted
                )
            })
            .map(|event| (event.event_type, event.body.as_str()))
            .collect();
        assert_that!(&(label_events)).is_equal_to(vec![
            (WorkItemEventType::LabelAdded, "Added label priority=high"),
            (
                WorkItemEventType::LabelUpdated,
                "Updated label priority=low",
            ),
            (
                WorkItemEventType::LabelDeleted,
                "Deleted label priority=low",
            ),
        ]);
    }

    #[tokio::test]
    async fn generic_label_mutations_reject_state_label_changes() {
        let (_temp, store) = test_store().await;
        let item = create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: "State label item".to_owned(),
                description: "State changes must use the item move workflow".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();
        let state_label_id = item
            .labels
            .iter()
            .find(|label| label.key == STATE_LABEL_KEY)
            .unwrap()
            .id;
        let priority = add_label(
            &store,
            "demo",
            item.id,
            "priority".to_owned(),
            Some("high".to_owned()),
            Some(item.version),
        )
        .await
        .unwrap();
        let priority_label_id = priority
            .labels
            .iter()
            .find(|label| label.key == "priority")
            .unwrap()
            .id;

        let add_state = add_label(
            &store,
            "demo",
            item.id,
            STATE_LABEL_KEY.to_owned(),
            Some("done".to_owned()),
            Some(priority.version),
        )
        .await
        .unwrap_err();
        assert_that!(&(add_state.to_string().contains("move the item"))).is_true();

        let update_state = update_label(
            &store,
            "demo",
            item.id,
            state_label_id,
            None,
            Some(Some("done".to_owned())),
            Some(priority.version),
        )
        .await
        .unwrap_err();
        assert_that!(&(update_state.to_string().contains("move the item"))).is_true();

        let rename_to_state = update_label(
            &store,
            "demo",
            item.id,
            priority_label_id,
            Some(STATE_LABEL_KEY.to_owned()),
            Some(Some("done".to_owned())),
            Some(priority.version),
        )
        .await
        .unwrap_err();
        assert_that!(&(rename_to_state.to_string().contains("move the item"))).is_true();

        let delete_state = delete_label(
            &store,
            "demo",
            item.id,
            state_label_id,
            Some(priority.version),
        )
        .await
        .unwrap_err();
        assert_that!(&(delete_state.to_string().contains("move the item"))).is_true();
    }
}
