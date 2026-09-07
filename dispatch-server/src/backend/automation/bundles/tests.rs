use assertr::prelude::*;
use tempfile::TempDir;

use super::policy::validate_yaml;
use crate::backend::projects::CreateProject;
use crate::backend::{
    automation::rules as automation_triggers,
    entities::{
        automation_trigger::{self, AutomationTrigger},
        personality::{self, Personality},
    },
    projects::repository::ProjectRepository,
    storage::Store,
};
use dispatch_types::{AutomationActivation, AutomationEffect, BundleDiffOperation};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("dispatch.sqlite3"), 1)
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
    (temp, store)
}

#[test]
fn reference_engineering_review_bundle_is_valid() {
    let bundle = validate_yaml(include_str!(
        "../../../../../examples/automation/engineering-review.yaml"
    ))
    .unwrap();
    assert_that!(&(bundle.manifest.bundle_key)).is_equal_to("engineering-review");
    assert_that!(&(bundle.manifest.personalities.len())).is_equal_to(2);
    assert_that!(&(bundle.manifest.automations.len())).is_equal_to(10);
}

#[test]
fn bundle_rejects_unknown_fields() {
    let error =
        validate_yaml("schema_version: 1\nbundle_key: demo\ndisplay_name: Demo\nunknown: true\n")
            .unwrap_err();
    assert_that!(&(error.to_string().contains("unknown field"))).is_true();
}

#[tokio::test]
async fn semantically_unchanged_apply_does_not_create_revisions() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let yaml = include_str!("../../../../../examples/automation/engineering-review.yaml");
    let first = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .apply("demo", yaml, None)
        .await
        .unwrap();
    let rules = crate::backend::automation::rules::tests::service(&store)
        .list("demo")
        .await
        .unwrap();
    let revision_ids = rules
        .iter()
        .map(|rule| rule.current_revision_id)
        .collect::<Vec<_>>();

    let second = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .apply("demo", yaml, Some(first.diff.manifest_hash.as_str()))
        .await
        .unwrap();
    assert_that!(
        &(second
            .diff
            .objects
            .iter()
            .all(|object| object.operation == BundleDiffOperation::Unchanged))
    )
    .with_detail_message(format!("{:#?}", second.diff.objects))
    .is_true();
    let after = crate::backend::automation::rules::tests::service(&store)
        .list("demo")
        .await
        .unwrap();
    assert_that!(&(revision_ids)).is_equal_to(
        after
            .iter()
            .map(|rule| rule.current_revision_id)
            .collect::<Vec<_>>(),
    );

    let exported = crate::backend::automation::bundles::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .export("demo", "engineering-review")
    .await
    .unwrap();
    let exported = validate_yaml(&exported).unwrap();
    assert_that!(&(exported.manifest_hash)).is_equal_to(first.diff.manifest_hash);
}

#[tokio::test]
async fn installed_bundle_inventory_removal_and_reinstall_are_consistent() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let yaml = include_str!("../../../../../examples/automation/engineering-review.yaml");
    let first = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .apply("demo", yaml, None)
        .await
        .unwrap();

    let installed = crate::backend::automation::bundles::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .list_installed("demo")
    .await
    .unwrap();
    assert_that!(&(installed.len())).is_equal_to(1);
    assert_that!(&(installed[0].bundle_key)).is_equal_to("engineering-review");
    assert_that!(&(installed[0].automation_count)).is_equal_to(10);
    assert_that!(&(installed[0].personality_count)).is_equal_to(2);

    let stale = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .remove("demo", "engineering-review", Some("stale"))
        .await
        .unwrap_err();
    assert_that!(&(stale.to_string().contains("bundle hash changed"))).is_true();
    assert_that!(
        &(crate::backend::automation::bundles::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list_installed("demo")
        .await
        .unwrap()
        .len())
    )
    .is_equal_to(1);

    let removed = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .remove(
            "demo",
            "engineering-review",
            Some(&first.diff.manifest_hash),
        )
        .await
        .unwrap();
    assert_that!(&(removed.status)).is_equal_to("removed");
    assert_that!(&(removed.diff.has_deletions)).is_true();
    assert_that!(
        &(crate::backend::automation::bundles::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list_installed("demo")
        .await
        .unwrap()
        .is_empty())
    )
    .is_true();
    assert_that!(
        &(crate::backend::automation::bundles::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .export("demo", "engineering-review")
        .await
        .unwrap_err()
        .to_string()
        .contains("has not been applied"))
    )
    .is_true();

    let reapplied = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .apply("demo", yaml, None)
        .await
        .unwrap();
    assert_that!(&(reapplied.status)).is_equal_to("applied");
    assert_that!(
        &(crate::backend::automation::bundles::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list_installed("demo")
        .await
        .unwrap()
        .len())
    )
    .is_equal_to(1);
}

#[tokio::test]
async fn bundle_removal_is_atomic_when_an_outside_rule_uses_a_managed_personality() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let yaml = include_str!("../../../../../examples/automation/engineering-review.yaml");
    let applied = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .apply("demo", yaml, None)
        .await
        .unwrap();
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    let reviewer = Personality::find()
        .filter(personality::Column::ProjectId.eq(project_id))
        .filter(personality::Column::ManagedBundleKey.eq("engineering-review"))
        .filter(personality::Column::ManagedObjectKey.eq("reviewer"))
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    crate::backend::automation::rules::tests::service_with_events(&store, event_bus.clone())
        .create(
            "demo",
            automation_triggers::model::CreateAutomationTrigger {
                name: "Outside reviewer".to_owned(),
                enabled: false,
                activation: AutomationActivation::WorkItem,
                effect: AutomationEffect::ConsumeWork,
                schedule: "@every 15s".to_owned(),
                tool_name: None,
                mutability: crate::shared::view_models::AutomationRunMutability::ReadOnly,
                personality_id: Some(reviewer.id),
                prompt: "Outside rule".to_owned(),
                work_item_selector: Some(
                    crate::shared::view_models::default_automation_work_item_selector(),
                ),
                priority: 0,
            },
        )
        .await
        .unwrap();

    let error = crate::backend::automation::bundles::tests::service(&store, event_bus.clone())
        .remove(
            "demo",
            "engineering-review",
            Some(&applied.diff.manifest_hash),
        )
        .await
        .unwrap_err();
    assert_that!(&(error.to_string().contains("outside the bundle references"))).is_true();
    assert_that!(
        &(crate::backend::automation::bundles::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .list_installed("demo")
        .await
        .unwrap()
        .len())
    )
    .is_equal_to(1);
    assert_that!(
        &(AutomationTrigger::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .filter(automation_trigger::Column::ManagedBundleKey.eq("engineering-review"))
            .all(store.db().as_ref())
            .await
            .unwrap()
            .len())
    )
    .is_equal_to(10);
}

pub(crate) fn service(
    store: &Store,
    events: crate::backend::events::UiEventBus,
) -> std::sync::Arc<super::service::BundleService> {
    use std::sync::Arc;
    Arc::new(super::service::BundleService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(super::repository::BundleRepository::new(
            Arc::new(crate::backend::automation::rules::repository::RuleRepository),
            Arc::new(crate::backend::automation::personalities::repository::PersonalityRepository),
        )),
        crate::backend::automation::rules::tests::service_with_events(store, events.clone()),
        crate::backend::automation::personalities::tests::service(store),
        events,
    ))
}

#[tokio::test]
async fn bundle_history_failure_rolls_back_all_domain_writes_and_notifications() {
    use crate::backend::entities::{
        automation_bundle_apply::AutomationBundleApply,
        automation_trigger_revision::AutomationTriggerRevision,
        personality_revision::PersonalityRevision,
    };
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, store) = test_store().await;
    let events = crate::backend::events::UiEventBus::new();
    let bundles = service(&store, events.clone());
    let mut notifications = events.subscribe();
    let yaml = include_str!("../../../../../examples/automation/engineering-review.yaml");
    macro_rules! snapshot {
        () => {{
            (
                AutomationTrigger::find()
                    .all(store.db().as_ref())
                    .await
                    .unwrap(),
                Personality::find().all(store.db().as_ref()).await.unwrap(),
                AutomationTriggerRevision::find()
                    .all(store.db().as_ref())
                    .await
                    .unwrap(),
                PersonalityRevision::find()
                    .all(store.db().as_ref())
                    .await
                    .unwrap(),
                AutomationBundleApply::find()
                    .all(store.db().as_ref())
                    .await
                    .unwrap(),
            )
        }};
    }
    let original = snapshot!();
    let fail_history = "CREATE TRIGGER fail_bundle_history BEFORE INSERT ON automation_bundle_applies BEGIN SELECT RAISE(FAIL, 'history unavailable'); END";
    store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            fail_history.to_owned(),
        ))
        .await
        .unwrap();
    assert_that!(&bundles.apply("demo", yaml, None).await.is_err()).is_true();
    assert_that!(&snapshot!()).is_equal_to(original);
    assert_that!(&notifications.try_recv().is_err()).is_true();
    store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_bundle_history".to_owned(),
        ))
        .await
        .unwrap();
    let applied = bundles.apply("demo", yaml, None).await.unwrap();
    assert_that!(&matches!(
        notifications.try_recv().unwrap(),
        dispatch_types::UiEvent::AutomationChanged { .. }
    ))
    .is_true();
    assert_that!(&notifications.try_recv().is_err()).is_true();
    let installed = snapshot!();
    store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            fail_history.to_owned(),
        ))
        .await
        .unwrap();
    let changed = yaml
        .replace(
            "Inspect the repository carefully",
            "Inspect all modules carefully",
        )
        .replace(
            "Plan an engineering review of",
            "Plan a thorough engineering review of",
        );
    assert_that!(
        &bundles
            .apply("demo", &changed, Some(&applied.diff.manifest_hash))
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&snapshot!()).is_equal_to(installed.clone());
    assert_that!(
        &bundles
            .remove(
                "demo",
                "engineering-review",
                Some(&applied.diff.manifest_hash)
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&snapshot!()).is_equal_to(installed);
    assert_that!(&notifications.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn current_bundle_apply_checks_deletions_and_nested_rule_scope_on_one_connection() {
    let (_temp, store) = test_store().await;
    let events = crate::backend::events::UiEventBus::new();
    let bundles = service(&store, events.clone());
    let mut notifications = events.subscribe();
    let yaml = include_str!("../../../../../examples/automation/engineering-review.yaml");
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        crate::backend::items::CreateWorkItem {
            title: "Existing item".into(),
            description: "Exists before event-driven bundle creation".into(),
            state: "open".into(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let cursor =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .events("demo", Some(item.id), None)
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == dispatch_types::WorkItemEventType::ItemCreated)
            .map(|event| event.id)
            .max();
    // Event-driven creation resolves its cursor and newly inserted personalities on the caller's transaction.
    let yaml = yaml.replacen("activation: work_item", "activation: work_item_created", 1);
    let applied = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        bundles.apply_current("demo", &yaml, false),
    )
    .await
    .unwrap()
    .unwrap();
    let rules = crate::backend::automation::rules::tests::service(&store)
        .list("demo")
        .await
        .unwrap();
    let campaign = rules
        .iter()
        .find(|rule| rule.managed_object_key.as_deref() == Some("planner"))
        .unwrap();
    assert_that!(&campaign.activation).is_equal_to(AutomationActivation::WorkItemCreated);
    assert_that!(&campaign.last_event_id).is_equal_to(cursor);
    assert_that!(&notifications.try_recv().is_ok()).is_true();
    let empty = "schema_version: 1\nbundle_key: engineering-review\ndisplay_name: Empty\npersonalities: []\nautomations: []\n";
    let error = bundles
        .apply_current("demo", empty, false)
        .await
        .unwrap_err();
    assert_that!(&error.to_string()).contains("confirm deletions");
    assert_that!(&bundles.list_installed("demo").await.unwrap()[0].manifest_hash)
        .is_equal_to(applied.diff.manifest_hash);
    assert_that!(&notifications.try_recv().is_err()).is_true();
    let removed = bundles.apply_current("demo", empty, true).await.unwrap();
    assert_that!(&removed.diff.has_deletions).is_true();
    assert_that!(&bundles.list_installed("demo").await.unwrap()[0].automation_count).is_equal_to(0);
    assert_that!(&notifications.try_recv().is_ok()).is_true();
    assert_that!(&notifications.try_recv().is_err()).is_true();
}
