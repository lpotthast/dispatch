use super::*;
use crate::backend::comments::{CommentTarget, tests::application};
use assertr::prelude::*;
use axum::Extension;
use dispatch_types::{AddCommentRequest, AuthorType, CommentView, UiEvent, WorkItemEventType};
use reqwest::{Client, StatusCode};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use std::time::Duration;

#[tokio::test]
async fn service_json_and_crudkit_comments_share_history_versions_and_notifications() {
    let mut fixtures = vec![];
    let mut urls = vec![];
    let mut events = vec![];
    let mut servers = tokio::task::JoinSet::new();
    for _ in 0..3 {
        let fixture = application().await;
        let app = &fixture.1;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        urls.push(format!("http://{}", listener.local_addr().unwrap()));
        let router = api::routes()
            .merge(crud::routes())
            .layer(Extension(app.state.clone()))
            .layer(Extension(app.contexts.comment.clone()));
        servers.spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        events.push(app.state.events.subscribe());
        fixtures.push(fixture);
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let request = AddCommentRequest {
        author_type: AuthorType::User,
        author_name: Some("Operator".into()),
        body: "Shared context".into(),
    };
    fixtures[0]
        .1
        .state
        .comments
        .add(
            CommentTarget::ProjectItem {
                project: "demo",
                item_id: fixtures[0].2,
            },
            request.clone(),
            Default::default(),
        )
        .await
        .unwrap();
    let response = client
        .post(format!(
            "{}/api/projects/demo/items/{}/comments",
            urls[1], fixtures[1].2
        ))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let response = client.post(format!("{}/api/comments/crud/create-one", urls[2])).json(&serde_json::json!({
        "entity": { "work_item_id": fixtures[2].2, "author_type": "user", "author_name": "Operator", "body": "Shared context" }
    })).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let created: serde_json::Value = response.json().await.unwrap();
    for (index, (_, app, item_id, _)) in fixtures.iter().enumerate() {
        let response = client
            .get(format!(
                "{}/api/projects/demo/items/{item_id}/comments",
                urls[index]
            ))
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(StatusCode::OK);
        let comments = response.json::<Vec<CommentView>>().await.unwrap();
        assert_that!(&comments.len()).is_equal_to(1);
        assert_that!(&comments[0].author_type).is_equal_to(AuthorType::User);
        assert_that!(&comments[0].author_name).is_equal_to(request.author_name.clone());
        assert_that!(&comments[0].body).is_equal_to(request.body.clone());
        assert_that!(&comments[0].created_at.is_empty()).is_false();
        let item = crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new(),
        )
        .get("demo", *item_id)
        .await
        .unwrap();
        assert_that!(&item.version).is_equal_to(2);
        assert_that!(&item.comment_count).is_equal_to(1);
        let history = crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", Some(*item_id), None)
        .await
        .unwrap();
        let added = history
            .iter()
            .filter(|event| event.event_type == WorkItemEventType::CommentAdded)
            .collect::<Vec<_>>();
        assert_that!(&added.len()).is_equal_to(1);
        assert_that!(&added[0].body).is_equal_to("Added comment");
        assert_that!(&added[0].actor_type).is_none();
        assert_that!(&matches!(
            events[index].try_recv().unwrap(),
            UiEvent::CommentChanged { .. }
        ))
        .is_true();
        assert_that!(&events[index].try_recv().is_err()).is_true();
    }
    let app = &fixtures[2].1;
    let item_id = fixtures[2].2;
    let mut update = created["entity"].clone();
    update["body"] = "Edited context".into();
    let response = client
        .post(format!("{}/api/comments/crud/update-one", urls[2]))
        .json(&serde_json::json!({ "condition": null, "entity": update }))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let comments = app.state.comments.list("demo", item_id).await.unwrap();
    assert_that!(&comments[0].body).is_equal_to("Edited context");
    assert_that!(&comments[0].created_at)
        .is_equal_to(created["entity"]["created_at"].as_str().unwrap());
    assert_that!(&matches!(
        events[2].try_recv().unwrap(),
        UiEvent::CommentChanged { .. }
    ))
    .is_true();
    update["body"] = "  ".into();
    let response = client
        .post(format!("{}/api/comments/crud/update-one", urls[2]))
        .json(&serde_json::json!({ "condition": null, "entity": update }))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::UNPROCESSABLE_ENTITY);
    assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()[0].body)
        .is_equal_to("Edited context");
    assert_that!(&events[2].try_recv().is_err()).is_true();
    use crudkit_core::Id;
    let id =
        crate::backend::entities::comment::CommentId { id: comments[0].id }.to_serializable_id();
    let response = client
        .post(format!("{}/api/comments/crud/delete-by-id", urls[2]))
        .json(&serde_json::json!({ "id": id }))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()).is_empty();
    assert_that!(&matches!(
        events[2].try_recv().unwrap(),
        UiEvent::CommentChanged { .. }
    ))
    .is_true();
    // Maintenance does not fabricate another comment-add event or alter the item workflow version.
    assert_that!(
        &crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new()
        )
        .get("demo", item_id)
        .await
        .unwrap()
        .version
    )
    .is_equal_to(2);
}

#[tokio::test]
async fn crudkit_comment_creation_rolls_back_on_history_failure() {
    let (_temp, app, item_id, _) = application().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = crud::routes().layer(Extension(app.contexts.comment.clone()));
    let mut server = tokio::task::JoinSet::new();
    server.spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER fail_comment_history BEFORE INSERT ON work_item_events WHEN NEW.event_type = 'comment_added' BEGIN SELECT RAISE(ABORT, 'injected history failure'); END".to_owned())).await.unwrap();
    let mut events = app.state.events.subscribe();
    let response = Client::builder().timeout(Duration::from_secs(5)).build().unwrap()
        .post(format!("{url}/api/comments/crud/create-one")).json(&serde_json::json!({
            "entity": { "work_item_id": item_id, "author_type": "agent", "author_name": null, "body": "Roll back" }
        })).send().await.unwrap();
    assert_that!(&response.status().is_success()).is_false();
    assert_that!(&app.state.comments.list("demo", item_id).await.unwrap()).is_empty();
    assert_that!(
        &crate::backend::items::tests::service(
            &app.state.store,
            crate::backend::events::UiEventBus::new()
        )
        .get("demo", item_id)
        .await
        .unwrap()
        .version
    )
    .is_equal_to(1);
    assert_that!(&events.try_recv().is_err()).is_true();
}
