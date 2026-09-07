use super::{api, crud};
use crate::backend::{
    comments::tests::application, items::CreateWorkItem, projects::ProjectReference,
};
use assertr::prelude::*;
use axum::Extension;
use dispatch_types::{UpdateWorkItemRequest, WorkItemEventType, WorkItemView};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[tokio::test]
async fn service_json_and_crudkit_create_and_update_share_history_and_notifications() {
    let mut fixtures = Vec::new();
    let mut urls = Vec::new();
    let mut receivers = Vec::new();
    let mut servers = tokio::task::JoinSet::new();
    for _ in 0..3 {
        let fixture = application().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        urls.push(format!("http://{}", listener.local_addr().unwrap()));
        let router = api::routes()
            .merge(crud::routes())
            .layer(Extension(fixture.1.state.clone()))
            .layer(Extension(fixture.1.contexts.work_item.clone()));
        servers.spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        receivers.push(fixture.1.state.events.subscribe());
        fixtures.push(fixture);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let input = serde_json::json!({"title":"Shared item", "description":"Shared details", "state":"open", "initial_labels":[{"key":"priority", "value":"high"}]});
    let first = fixtures[0]
        .1
        .state
        .item_creation
        .create(
            ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Shared item".into(),
                description: "Shared details".into(),
                state: "open".into(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![dispatch_types::CreateWorkItemLabelRequest {
                    key: "priority".into(),
                    value: Some("high".into()),
                }],
            },
            Default::default(),
        )
        .await
        .unwrap();
    let response = client
        .post(format!("{}/api/projects/demo/items", urls[1]))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let second = response.json::<WorkItemView>().await.unwrap();
    let mut entity = input;
    entity["project_id"] = first.project_id.into();
    let response = client
        .post(format!("{}/api/work_items/crud/create-one", urls[2]))
        .json(&serde_json::json!({"entity":entity}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let created = response.json::<serde_json::Value>().await.unwrap();
    let third_id = created["entity"]["id"].as_i64().unwrap();
    let ids = [first.id, second.id, third_id];
    for (index, (_, app, _, _)) in fixtures.iter().enumerate() {
        let item = app.state.items.get("demo", ids[index]).await.unwrap();
        assert_that!(&item.title).is_equal_to("Shared item");
        assert_that!(&item.version).is_equal_to(1);
        assert_that!(
            &item
                .labels
                .iter()
                .any(|label| label.key == "priority" && label.value.as_deref() == Some("high"))
        )
        .is_true();
        assert_that!(&receivers[index].try_recv().is_ok()).is_true();
        assert_that!(&receivers[index].try_recv().is_err()).is_true();
    }
    fixtures[0]
        .1
        .state
        .items
        .update(
            ProjectReference::Name("demo"),
            ids[0],
            UpdateWorkItemRequest {
                title: Some("Edited item".into()),
                expect_version: Some(1),
                ..Default::default()
            },
            Default::default(),
        )
        .await
        .unwrap();
    let response = client
        .patch(format!("{}/api/projects/demo/items/{}", urls[1], ids[1]))
        .json(&serde_json::json!({"title":"Edited item", "expect_version":1}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "id".into(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(third_id),
    })]);
    let mut update = created["entity"].clone();
    update["title"] = "Edited item".into();
    let response = client
        .post(format!("{}/api/work_items/crud/update-one", urls[2]))
        .json(&serde_json::json!({"condition":condition, "entity":update}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    for (index, (_, app, _, _)) in fixtures.iter().enumerate() {
        let item = app.state.items.get("demo", ids[index]).await.unwrap();
        assert_that!(&item.title).is_equal_to("Edited item");
        assert_that!(&item.version).is_equal_to(2);
        let history = app
            .state
            .items
            .events("demo", Some(item.id), None)
            .await
            .unwrap();
        assert_that!(
            &history
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>()
        )
        .is_equal_to(vec![
            WorkItemEventType::ItemCreated,
            WorkItemEventType::ItemUpdated,
        ]);
        assert_that!(&receivers[index].try_recv().is_ok()).is_true();
        assert_that!(&receivers[index].try_recv().is_err()).is_true();
    }
}

#[tokio::test]
async fn failed_item_history_rolls_back_creation_update_and_delete_on_one_connection() {
    let (_temp, app, item_id, _) = application().await;
    let before = app.state.items.get("demo", item_id).await.unwrap();
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER fail_item_history BEFORE INSERT ON work_item_events BEGIN SELECT RAISE(ABORT, 'injected history failure'); END".to_owned())).await.unwrap();
    let result = app
        .state
        .item_creation
        .create(
            ProjectReference::Name("demo"),
            CreateWorkItem {
                title: "Rolled back".into(),
                description: "No partial labels or origin".into(),
                state: "open".into(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: vec![dispatch_types::CreateWorkItemLabelRequest {
                    key: "new-key".into(),
                    value: None,
                }],
            },
            Default::default(),
        )
        .await;
    assert_that!(&result.is_err()).is_true();
    assert_that!(&app.state.items.list("demo", None).await.unwrap().len()).is_equal_to(2);
    let result = app
        .state
        .items
        .update(
            ProjectReference::Name("demo"),
            item_id,
            UpdateWorkItemRequest {
                title: Some("Rolled back".into()),
                state: Some("review".into()),
                expect_version: Some(before.version),
                ..Default::default()
            },
            Default::default(),
        )
        .await;
    assert_that!(&result.is_err()).is_true();
    let result = app
        .state
        .items
        .delete(ProjectReference::Name("demo"), item_id)
        .await;
    assert_that!(&result.is_err()).is_true();
    assert_that!(&app.state.items.get("demo", item_id).await.unwrap()).is_equal_to(before);
    assert_that!(&events.try_recv().is_err()).is_true();
    let row = app
        .state
        .store
        .db()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM label_keys WHERE label_key = 'new-key'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&row.try_get::<i64>("", "count").unwrap()).is_equal_to(0);
}
