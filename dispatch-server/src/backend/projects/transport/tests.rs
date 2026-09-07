use crate::backend::{
    application::Application,
    projects::{CreateProject, UpdateProjectSettings},
    storage::Store,
};
use assertr::prelude::*;
use axum::Extension;
use dispatch_types::{ProjectView, UiEvent};
use reqwest::{Client, StatusCode};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use std::time::Duration;
use tempfile::TempDir;

fn comparable(mut view: ProjectView) -> ProjectView {
    view.created_at.clear();
    view.updated_at.clear();
    view.path_checked_at = None;
    view
}

fn notification_kinds(
    receiver: &mut tokio::sync::broadcast::Receiver<UiEvent>,
) -> Vec<&'static str> {
    let mut kinds = vec![];
    while let Ok(event) = receiver.try_recv() {
        kinds.push(match event {
            UiEvent::ProjectListChanged { .. } => "project-list",
            UiEvent::ProjectChanged { .. } => "project",
            _ => panic!("unexpected project creation event: {event:?}"),
        });
    }
    kinds
}

async fn catalog_counts(store: &Store) -> Vec<i64> {
    let mut counts = vec![];
    for table in [
        "label_keys",
        "personalities",
        "work_item_states",
        "swim_lanes",
        "automation_triggers",
        "work_item_events",
    ] {
        let row = store
            .db()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT COUNT(*) AS count FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap();
        counts.push(row.try_get("", "count").unwrap());
    }
    counts
}

#[tokio::test]
async fn project_service_forms_json_and_crudkit_share_configuration_and_defaults() {
    let temp = TempDir::new().unwrap();
    let mut apps = vec![];
    let mut servers = tokio::task::JoinSet::new();
    let mut urls = vec![];
    let mut events = vec![];
    for index in 0..3 {
        let store =
            Store::open_with_max_connections(temp.path().join(format!("app-{index}.sqlite3")), 1)
                .await
                .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Application::from_store(store, url.clone());
        let router = super::http::routes()
            .merge(super::api::routes())
            .merge(super::crud::routes())
            .layer(Extension(app.state.clone()))
            .layer(Extension(app.contexts.project.clone()));
        servers.spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        events.push(app.state.events.subscribe());
        apps.push(app);
        urls.push(url);
    }
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    apps[0]
        .state
        .projects
        .create(CreateProject {
            name: "demo".into(),
            display_name: Some("  Demo Project  ".into()),
            path: temp.path().to_owned(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    let response = client
        .post(format!("{}/projects", urls[1]))
        .form(&[
            ("name", "demo"),
            ("display_name", "  Demo Project  "),
            ("path", temp.path().to_str().unwrap()),
        ])
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::SEE_OTHER);
    let response = client.post(format!("{}/api/projects/crud/create-one", urls[2])).json(&serde_json::json!({
        "entity": {"name":"demo", "display_name":"  Demo Project  ", "path": temp.path(), "default_agent_model":null, "default_agent_reasoning_effort":null}
    })).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let created: serde_json::Value = response.json().await.unwrap();
    let expected = comparable(apps[0].state.projects.get("demo").await.unwrap());
    let expected_counts = catalog_counts(&apps[0].state.store).await;
    for (index, app) in apps.iter().enumerate() {
        let response = client
            .get(format!("{}/api/projects/demo", urls[index]))
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(StatusCode::OK);
        assert_that!(&comparable(response.json::<ProjectView>().await.unwrap()))
            .is_equal_to(expected.clone());
        assert_that!(&catalog_counts(&app.state.store).await).is_equal_to(expected_counts.clone());
        assert_that!(&notification_kinds(&mut events[index]))
            .is_equal_to(vec!["project-list", "project"]);
    }

    apps[0]
        .state
        .projects
        .update_settings(
            "demo",
            UpdateProjectSettings {
                auto_commit: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let response = client
        .post(format!("{}/projects/demo/settings/auto-commit", urls[1]))
        .form(&[("enabled", "false")])
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::SEE_OTHER);
    let mut update = created["entity"].clone();
    update["auto_commit"] = false.into();
    let response = client
        .post(format!("{}/api/projects/crud/update-one", urls[2]))
        .json(&serde_json::json!({"condition":null, "entity":update}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let expected = comparable(apps[0].state.projects.get("demo").await.unwrap());
    for app in &apps {
        assert_that!(&comparable(app.state.projects.get("demo").await.unwrap()))
            .is_equal_to(expected.clone());
    }
    // Invalid policy remains a transport validation error, without partial state or success events.
    notification_kinds(&mut events[2]);
    update["max_code_edit_agents"] = 2.into();
    let response = client
        .post(format!("{}/api/projects/crud/update-one", urls[2]))
        .json(&serde_json::json!({"condition":null, "entity":update}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::UNPROCESSABLE_ENTITY);
    assert_that!(&notification_kinds(&mut events[2])).is_empty();
    assert_that!(&comparable(
        apps[2].state.projects.get("demo").await.unwrap()
    ))
    .is_equal_to(expected);
    update["max_code_edit_agents"] = 1.into();
    update["path"] = serde_json::Value::Null;
    let response = client
        .post(format!("{}/api/projects/crud/update-one", urls[2]))
        .json(&serde_json::json!({"condition":null, "entity":update}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(StatusCode::OK);
    let project = apps[2].state.projects.get("demo").await.unwrap();
    assert_that!(&project.path).is_none();
    assert_that!(&project.path_exists).is_false();
}

#[tokio::test]
async fn crudkit_failure_during_default_seeding_leaves_no_project_or_success_notification() {
    use crudkit_rs::{
        create::{CreateOne, create_one},
        prelude::RequestContext,
    };
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("db.sqlite3"), 1)
        .await
        .unwrap();
    let app = Application::from_store(store, "http://127.0.0.1:4000".into());
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_personality BEFORE INSERT ON personalities BEGIN SELECT RAISE(ABORT, 'reject default personality'); END".to_owned())).await.unwrap();
    let mut events = app.state.events.subscribe();
    let entity = serde_json::from_value(serde_json::json!({"name":"demo", "display_name":"Demo", "path":temp.path(), "default_agent_model":null, "default_agent_reasoning_effort":null})).unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        create_one::<super::crud::CrudProjectResource>(
            RequestContext::unauthenticated(),
            app.contexts.project.clone(),
            CreateOne { entity },
        ),
    )
    .await
    .unwrap();
    assert_that!(&result.is_err()).is_true();
    assert_that!(&app.state.projects.list().await.unwrap()).is_empty();
    assert_that!(&catalog_counts(&app.state.store).await).is_equal_to(vec![0, 0, 0, 0, 0, 0]);
    assert_that!(&events.try_recv().is_err()).is_true();
}
