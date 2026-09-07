use crate::backend::{
    events::UiEventBus,
    items,
    projects::repository::ProjectRepository,
    storage::{Store, TransactionManager},
};
use std::sync::Arc;
pub(crate) fn service(store: &Store, events: UiEventBus) -> Arc<super::service::ProductionService> {
    Arc::new(super::service::ProductionService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(items::repository::ItemRepository),
        items::creation::tests::service(store, events.clone()),
        Arc::new(super::repository::ProductionRepository),
        events,
    ))
}

#[tokio::test]
async fn producer_rolls_back_evaluation_item_and_origin_when_final_link_fails() {
    use crate::backend::{
        automation::rules::model::CreateAutomationTrigger, comments::tests::application,
    };
    use assertr::prelude::*;
    use dispatch_types::{AutomationActivation, AutomationEffect, AutomationRunMutability};
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = application().await;
    let rule = app
        .state
        .rules
        .create(
            "demo",
            CreateAutomationTrigger {
                name: "Atomic producer".into(),
                enabled: true,
                activation: AutomationActivation::Manual,
                effect: AutomationEffect::ProduceWork,
                schedule: "@every 15s".into(),
                tool_name: None,
                mutability: AutomationRunMutability::ReadOnly,
                personality_id: None,
                prompt: "Produce work atomically".into(),
                work_item_selector: None,
                priority: 0,
            },
        )
        .await
        .unwrap();
    let before = app.state.items.list("demo", None).await.unwrap();
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER fail_production_link BEFORE UPDATE OF work_item_id ON automation_evaluations BEGIN SELECT RAISE(ABORT, 'injected final linkage failure'); END".to_owned())).await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.state.production.produce("demo", &rule),
    )
    .await
    .unwrap();
    assert_that!(&result.is_err()).is_true();
    assert_that!(&app.state.items.list("demo", None).await.unwrap()).is_equal_to(before);
    let row = app
        .state
        .store
        .db()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM automation_evaluations".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&row.try_get::<i64>("", "count").unwrap()).is_equal_to(0);
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER fail_production_link".to_owned(),
        ))
        .await
        .unwrap();
    let item = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.state.production.produce("demo", &rule),
    )
    .await
    .unwrap()
    .unwrap();
    assert_that!(&item.origin.unwrap().producing_evaluation_id.is_some()).is_true();
    assert_that!(&events.try_recv().is_ok()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
}
