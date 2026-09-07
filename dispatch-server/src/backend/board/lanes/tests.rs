use assertr::prelude::*;
use tempfile::TempDir;

use super::{policy::*, repository::LaneRepository, service::LaneService};
use crate::backend::projects::CreateProject;
use crate::backend::{
    projects::repository::ProjectRepository,
    storage::{Store, TransactionManager},
};
use std::sync::Arc;

#[tokio::test]
async fn default_swim_lanes_configure_item_creation() {
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

    let flags = service(&store)
        .list("demo")
        .await
        .unwrap()
        .into_iter()
        .map(|lane| (lane.identifier, lane.can_create_items))
        .collect::<Vec<_>>();

    assert_that!(&(flags)).is_equal_to(vec![
        ("idea".to_owned(), true),
        ("open".to_owned(), true),
        ("in_progress".to_owned(), false),
        ("done".to_owned(), false),
    ]);
}

#[tokio::test]
async fn default_swim_lanes_filter_by_matching_state() {
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

    let lanes = service(&store)
        .list("demo")
        .await
        .unwrap()
        .into_iter()
        .map(|lane| (lane.identifier, lane.filter, lane.item_order))
        .collect::<Vec<_>>();

    assert_that!(&(lanes)).is_equal_to(vec![
        (
            "idea".to_owned(),
            state_filter("idea"),
            DEFAULT_SWIM_LANE_ITEM_ORDER,
        ),
        (
            "open".to_owned(),
            state_filter("open"),
            DEFAULT_SWIM_LANE_ITEM_ORDER,
        ),
        (
            "in_progress".to_owned(),
            state_filter("in_progress"),
            DEFAULT_SWIM_LANE_ITEM_ORDER,
        ),
        (
            "done".to_owned(),
            state_filter("done"),
            DEFAULT_SWIM_LANE_ITEM_ORDER,
        ),
    ]);
}

fn service(store: &Store) -> LaneService {
    LaneService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(LaneRepository),
        crate::backend::events::UiEventBus::new(),
    )
}

fn fields(name: &str) -> super::model::LaneFields {
    super::model::LaneFields {
        identifier: "review".into(),
        name: name.into(),
        position: 50,
        filter: super::policy::state_filter("open"),
        item_order: dispatch_types::SwimLaneItemOrder::UpdatedDesc,
        can_create_items: true,
    }
}
#[tokio::test]
async fn service_and_crudkit_share_validation_mutations_and_committed_events() {
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/api/swim_lanes/crud",
        listener.local_addr().unwrap()
    );
    let router = super::transport::routes().layer(axum::Extension(app.contexts.swim_lane.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let mut events = app.state.events.subscribe();
    let created = app
        .state
        .lanes
        .create(project_id, fields(" Review "))
        .await
        .unwrap();
    assert_that!(&created.name).is_equal_to("Review");
    assert_that!(&matches!(events.try_recv().unwrap(),dispatch_types::UiEvent::SwimLaneChanged{project,..} if project=="demo")).is_true();
    app.state
        .lanes
        .delete(project_id, created.id)
        .await
        .unwrap();
    events.try_recv().unwrap();
    let response=client.post(format!("{url}/create-one")).json(&serde_json::json!({"entity":{"project_id":project_id,"identifier":"review","name":" Review ","position":50,"filter":serde_json::to_string(&super::policy::state_filter("open")).unwrap(),"item_order":"updated_desc","can_create_items":true}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let value = response.json::<serde_json::Value>().await.unwrap();
    let id = value["entity"]["id"].as_i64().unwrap();
    let listed = app.state.lanes.list_by_id(project_id).await.unwrap();
    let read = listed.iter().find(|r| r.id == id).unwrap();
    assert_that!(&read.name).is_equal_to(created.name.as_str());
    assert_that!(&read.identifier).is_equal_to(created.identifier.as_str());
    assert_that!(&matches!(events.try_recv().unwrap(),dispatch_types::UiEvent::SwimLaneChanged{project,..} if project=="demo")).is_true();
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "id".into(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(id),
    })]);
    let mut entity = value["entity"].clone();
    entity["name"] = " Reviewed ".into();
    let response = client
        .post(format!("{url}/update-one"))
        .json(&serde_json::json!({"condition":condition,"entity":entity}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    assert_that!(
        &app.state
            .lanes
            .list_by_id(project_id)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap()
            .name
    )
    .is_equal_to("Reviewed");
    events.try_recv().unwrap();
    entity["identifier"] = "bad=state".into();
    let response = client
        .post(format!("{url}/update-one"))
        .json(&serde_json::json!({"condition":condition,"entity":entity}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert_that!(
        &app.state
            .lanes
            .update(
                project_id,
                id,
                super::model::LaneFields {
                    identifier: "bad=state".into(),
                    ..fields("Bad")
                }
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    let response = client
        .post(format!("{url}/delete-one"))
        .json(&serde_json::json!({"condition":condition}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    assert_that!(
        &app.state
            .lanes
            .list_by_id(project_id)
            .await
            .unwrap()
            .into_iter()
            .any(|r| r.id == id)
    )
    .is_false();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
    server.abort();
}
#[tokio::test]
async fn failed_mutations_and_cross_project_scope_never_publish_success() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let created = app
        .state
        .lanes
        .create(project_id, fields("Review"))
        .await
        .unwrap();
    let mut events = app.state.events.subscribe();
    for (verb, operation) in [("INSERT", 0), ("UPDATE", 1), ("DELETE", 2)] {
        app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,format!("CREATE TRIGGER reject_configuration BEFORE {verb} ON swim_lanes BEGIN SELECT RAISE(ABORT,'injected configuration failure'); END"))).await.unwrap();
        let failed = match operation {
            0 => app
                .state
                .lanes
                .create(
                    project_id,
                    super::model::LaneFields {
                        identifier: "fresh".into(),
                        ..fields("Fresh")
                    },
                )
                .await
                .is_err(),
            1 => app
                .state
                .lanes
                .update(project_id, created.id, fields("Edited"))
                .await
                .is_err(),
            _ => app
                .state
                .lanes
                .delete(project_id, created.id)
                .await
                .is_err(),
        };
        assert_that!(&failed).is_true();
        app.state
            .store
            .db()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DROP TRIGGER reject_configuration".to_owned(),
            ))
            .await
            .unwrap();
        let rows = app.state.lanes.list_by_id(project_id).await.unwrap();
        assert_that!(&rows.len()).is_equal_to(5);
        assert_that!(&rows.iter().find(|r| r.id == created.id).unwrap().name).is_equal_to("Review");
        assert_that!(&events.try_recv().is_err()).is_true();
    }
    assert_that!(
        &app.state
            .lanes
            .update(project_id + 100, created.id, fields("Edited"))
            .await
            .is_err()
    )
    .is_true();
    assert_that!(
        &app.state
            .lanes
            .delete(project_id + 100, created.id)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .lanes
        .update(project_id, created.id, fields("Edited"))
        .await
        .unwrap();
    events.try_recv().unwrap();
    app.state
        .lanes
        .delete(project_id, created.id)
        .await
        .unwrap();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}
