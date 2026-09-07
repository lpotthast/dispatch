pub(crate) fn service(
    store: &crate::backend::storage::Store,
    launch: std::sync::Arc<crate::backend::automation::launch::service::LaunchService>,
    events: crate::backend::events::UiEventBus,
) -> super::service::SchedulerService {
    use std::sync::Arc;
    super::service::SchedulerService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(crate::backend::projects::repository::ProjectRepository::new(store.db())),
        Arc::new(super::repository::ScheduleRepository),
        Arc::new(crate::backend::items::repository::ItemRepository),
        crate::backend::items::claims::tests::service(store, events.clone()),
        crate::backend::automation::production::tests::service(store, events.clone()),
        crate::backend::runs::admission::tests::service(store),
        launch,
        events,
    )
}

#[tokio::test]
async fn schedule_failure_rolls_back_production_evaluation_and_item_history() {
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
                name: "Atomic scheduled producer".into(),
                enabled: true,
                activation: AutomationActivation::Manual,
                effect: AutomationEffect::ProduceWork,
                schedule: "@every 15s".into(),
                tool_name: None,
                mutability: AutomationRunMutability::ReadOnly,
                personality_id: None,
                prompt: "Produce within the evaluation transaction".into(),
                work_item_selector: None,
                priority: 0,
            },
        )
        .await
        .unwrap();
    app.state.rules.schedule("demo", rule.id).await.unwrap();
    let items = app.state.items.list("demo", None).await.unwrap();
    let history =
        serde_json::to_value(app.state.items.events("demo", None, None).await.unwrap()).unwrap();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER fail_schedule BEFORE UPDATE OF evaluation_count ON automation_triggers BEGIN SELECT RAISE(FAIL, 'schedule unavailable'); END".to_owned())).await.unwrap();
    let mut events = app.state.events.subscribe();
    let outcomes = app.state.scheduler.run_due(None, None).await.unwrap();
    assert_that!(&outcomes.len()).is_equal_to(1);
    assert_that!(&outcomes[0].error.is_some()).is_true();
    assert_that!(&app.state.items.list("demo", None).await.unwrap()).is_equal_to(items);
    assert_that!(
        &serde_json::to_value(app.state.items.events("demo", None, None).await.unwrap()).unwrap()
    )
    .is_equal_to(history);
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
            "DROP TRIGGER fail_schedule".to_owned(),
        ))
        .await
        .unwrap();
    app.state.rules.schedule("demo", rule.id).await.unwrap();
    let mut events = app.state.events.subscribe();
    let outcomes = app.state.scheduler.run_due(None, None).await.unwrap();
    assert_that!(&outcomes.len()).is_equal_to(1);
    assert_that!(&outcomes[0].error).is_none();
    let item = outcomes[0].work_item.as_ref().unwrap();
    assert_that!(&item.origin.as_ref().unwrap().producing_evaluation_id).is_some();
    assert_that!(&events.try_recv().is_ok()).is_true();
    assert_that!(&events.try_recv().is_ok()).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
}
