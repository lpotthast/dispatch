use super::transport::*;
use crate::backend::{crudkit_resources::tests::test_store, entities::label_key};
use assertr::prelude::*;
use crudkit_rs::prelude::*;
use dispatch_types::STATE_LABEL_KEY;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
#[tokio::test]
async fn label_key_create_normalizes_configuration_and_requires_persistence() {
    let (_temp, store, project_id) = test_store().await;
    let context = LabelKeyResourceContext {
        service: crate::backend::application::Application::from_store(
            store,
            "http://127.0.0.1:4000".into(),
        )
        .state
        .label_catalog
        .clone(),
    };
    let mut create = label_key::CreateModel {
        project_id,
        key: " priority ".to_owned(),
        accent_color: Some(" #AABBCC ".to_owned()),
        persistent: true,
    };

    LabelKeyLifetime::before_create(&mut create, &context, RequestContext::unauthenticated(), ())
        .await
        .unwrap();

    assert_that!(&(create.key)).is_equal_to("priority");
    assert_that!(&(create.accent_color.as_deref())).is_equal_to(Some("#aabbcc"));

    let mut volatile_create = label_key::CreateModel {
        project_id,
        key: "volatile".to_owned(),
        accent_color: None,
        persistent: false,
    };
    let error = LabelKeyLifetime::before_create(
        &mut volatile_create,
        &context,
        RequestContext::unauthenticated(),
        (),
    )
    .await
    .expect_err("unused label-key creation should require persistence");
    match error {
        HookError::UnprocessableEntity { reason } => {
            assert_that!(&(reason.contains("must be created as persistent"))).is_true();
        }
        other => panic!("expected unprocessable label-key create error, got {other:?}"),
    }
}

#[tokio::test]
async fn label_key_update_keeps_built_ins_persistent() {
    let (_temp, store, project_id) = test_store().await;
    let context = LabelKeyResourceContext {
        service: crate::backend::application::Application::from_store(
            store.clone(),
            "http://127.0.0.1:4000".into(),
        )
        .state
        .label_catalog
        .clone(),
    };
    let state = label_key::Entity::find()
        .filter(label_key::Column::ProjectId.eq(project_id))
        .filter(label_key::Column::Key.eq(STATE_LABEL_KEY))
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    let mut update = label_key::UpdateModel {
        accent_color: None,
        persistent: false,
    };

    let error = LabelKeyLifetime::before_update(
        &state,
        &mut update,
        &UpdateRequest { condition: None },
        &context,
        RequestContext::unauthenticated(),
        (),
    )
    .await
    .expect_err("built-in label keys should remain persistent");

    match error {
        HookError::UnprocessableEntity { reason } => {
            assert_that!(&(reason.contains("must remain persistent"))).is_true();
        }
        other => panic!("expected unprocessable label-key update error, got {other:?}"),
    }
}

#[tokio::test]
async fn catalog_service_and_crudkit_preserve_forgetting_and_permitted_operations() {
    use super::model::{CreateLabelKey, UpdateLabelKey};
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let mut events = app.state.events.subscribe();
    let key = app
        .state
        .label_catalog
        .create(
            project_id,
            CreateLabelKey {
                key: " priority ".into(),
                accent_color: Some(" #AABBCC ".into()),
                persistent: true,
            },
        )
        .await
        .unwrap();
    assert_that!(&key.key).is_equal_to("priority");
    assert_that!(&key.accent_color.as_deref()).is_equal_to(Some("#aabbcc"));
    events.try_recv().unwrap();
    app.state
        .label_catalog
        .update(
            project_id,
            key.id,
            UpdateLabelKey {
                accent_color: None,
                persistent: false,
            },
        )
        .await
        .unwrap();
    assert_that!(
        &app.state
            .label_catalog
            .exists(project_id, "priority")
            .await
            .unwrap()
    )
    .is_false();
    events.try_recv().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/api/label_keys/crud",
        listener.local_addr().unwrap()
    );
    let router = super::transport::routes().layer(axum::Extension(app.contexts.label_key.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let input = serde_json::json!({"entity":{"project_id":project_id,"key":" priority ","accent_color":" #AABBCC ","persistent":true}});
    let response = client
        .post(format!("{url}/create-one"))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let created = response.json::<serde_json::Value>().await.unwrap();
    let id = created["entity"]["id"].as_i64().unwrap();
    assert_that!(&created["entity"]["key"].as_str()).is_equal_to(Some(key.key.as_str()));
    assert_that!(&created["entity"]["accent_color"].as_str())
        .is_equal_to(key.accent_color.as_deref());
    events.try_recv().unwrap();
    let response = client
        .post(format!("{url}/create-one"))
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "id".into(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(id),
    })]);
    let response = client
        .post(format!("{url}/delete-one"))
        .json(&serde_json::json!({"condition":condition}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert_that!(&events.try_recv().is_err()).is_true();
    let response=client.post(format!("{url}/update-one")).json(&serde_json::json!({"condition":condition,"entity":{"accent_color":null,"persistent":false}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    assert_that!(
        &app.state
            .label_catalog
            .exists(project_id, "priority")
            .await
            .unwrap()
    )
    .is_false();
    assert_that!(&matches!(events.try_recv().unwrap(),dispatch_types::UiEvent::LabelKeyChanged{project,key,..} if project=="demo" && key=="priority")).is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    server.abort();
}

#[tokio::test]
async fn forgetting_failure_rolls_back_configuration_and_events_on_one_connection() {
    use super::model::{CreateLabelKey, UpdateLabelKey};
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let project_id = app.state.projects.id("demo").await.unwrap();
    let key = app
        .state
        .label_catalog
        .create(
            project_id,
            CreateLabelKey {
                key: "priority".into(),
                accent_color: Some("#aabbcc".into()),
                persistent: true,
            },
        )
        .await
        .unwrap();
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER reject_forgetting BEFORE DELETE ON label_keys BEGIN SELECT RAISE(ABORT,'injected forgetting failure'); END".to_owned())).await.unwrap();
    assert_that!(
        &app.state
            .label_catalog
            .update(
                project_id,
                key.id,
                UpdateLabelKey {
                    accent_color: None,
                    persistent: false
                }
            )
            .await
            .is_err()
    )
    .is_true();
    let stored = label_key::Entity::find_by_id(key.id)
        .one(app.state.store.db().as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_that!(&stored.persistent).is_true();
    assert_that!(&stored.accent_color.as_deref()).is_equal_to(Some("#aabbcc"));
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_forgetting".to_owned(),
        ))
        .await
        .unwrap();
    assert_that!(
        &app.state
            .label_catalog
            .update(
                project_id + 100,
                key.id,
                UpdateLabelKey {
                    accent_color: None,
                    persistent: false
                }
            )
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .label_catalog
        .update(
            project_id,
            key.id,
            UpdateLabelKey {
                accent_color: None,
                persistent: false,
            },
        )
        .await
        .unwrap();
    assert_that!(
        &app.state
            .label_catalog
            .exists(project_id, "priority")
            .await
            .unwrap()
    )
    .is_false();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}
