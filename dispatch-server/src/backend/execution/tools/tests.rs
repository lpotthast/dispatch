use assertr::prelude::*;
use tempfile::TempDir;

use crate::backend::storage::Store;
use dispatch_types::AgentToolName;
use std::path::PathBuf;

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    (temp, store)
}

#[tokio::test]
async fn explicit_tool_path_becomes_effective_path() {
    let (_temp, store) = test_store().await;

    let tool = crate::backend::execution::tools::tests::service(&store)
        .set_path(AgentToolName::Codex, PathBuf::from("/bin/echo"))
        .await
        .unwrap();

    assert_that!(&(tool.executable_path.as_deref())).is_equal_to(Some("/bin/echo"));
    assert_that!(&(tool.effective_path.as_deref())).is_equal_to(Some("/bin/echo"));
}

pub(crate) fn service(store: &Store) -> std::sync::Arc<super::service::ToolService> {
    use std::sync::Arc;
    Arc::new(super::service::ToolService::new(
        Arc::new(crate::backend::storage::TransactionManager::new(store)),
        Arc::new(super::repository::ToolRepository),
        Arc::new(super::runtime::ToolDiscovery::new(std::env::var_os("PATH"))),
        crate::backend::events::UiEventBus::new(),
    ))
}

#[tokio::test]
async fn service_and_crudkit_share_tool_configuration_and_discovery_metadata() {
    use crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    };
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let mut events = app.state.events.subscribe();
    let tool = app
        .state
        .tools
        .create(AgentToolName::Codex, Some("/explicit/codex".into()))
        .await
        .unwrap();
    assert_that!(&tool.effective_path.as_deref()).is_equal_to(Some("/explicit/codex"));
    events.try_recv().unwrap();
    app.state.tools.delete(tool.id).await.unwrap();
    events.try_recv().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/api/agent_tools/crud",
        listener.local_addr().unwrap()
    );
    let router = super::transport::routes().layer(axum::Extension(app.contexts.agent_tool.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let response=client.post(format!("{url}/create-one")).json(&serde_json::json!({"entity":{"tool_name":"codex","executable_path":"/explicit/codex"}})).send().await.unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    let created = response.json::<serde_json::Value>().await.unwrap();
    let id = created["entity"]["id"].as_i64().unwrap();
    events.try_recv().unwrap();
    assert_that!(&app.state.tools.resolve(AgentToolName::Codex).await.unwrap())
        .is_equal_to(PathBuf::from("/explicit/codex"));
    let discovered = app.state.tools.discover().await.unwrap().remove(0);
    events.try_recv().unwrap();
    assert_that!(&discovered.effective_path.as_deref()).is_equal_to(Some("/explicit/codex"));
    let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: "id".into(),
        operator: Operator::Equal,
        value: ConditionClauseValue::I64(id),
    })]);
    let response = client
        .post(format!("{url}/update-one"))
        .json(&serde_json::json!({"condition":condition,"entity":{"executable_path":null}}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    events.try_recv().unwrap();
    let updated = app.state.tools.list().await.unwrap().remove(0);
    assert_that!(&updated.discovered_path).is_equal_to(discovered.discovered_path.clone());
    assert_that!(&updated.last_discovered_at).is_equal_to(discovered.last_discovered_at.clone());
    assert_that!(&updated.effective_path).is_equal_to(discovered.discovered_path);
    let response = client
        .post(format!("{url}/delete-one"))
        .json(&serde_json::json!({"condition":condition}))
        .send()
        .await
        .unwrap();
    assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
    assert_that!(&app.state.tools.list().await.unwrap()).is_empty();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
    server.abort();
}

#[tokio::test]
async fn tool_configuration_failures_preserve_paths_and_publish_no_success() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let db = app.state.store.db();
    let mut events = app.state.events.subscribe();
    db.execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER reject_tool_create BEFORE INSERT ON agent_tools BEGIN SELECT RAISE(ABORT,'injected tool creation failure'); END".to_owned())).await.unwrap();
    assert_that!(&app.state.tools.discover().await.is_err()).is_true();
    assert_that!(&app.state.tools.list().await.unwrap()).is_empty();
    assert_that!(&events.try_recv().is_err()).is_true();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "DROP TRIGGER reject_tool_create".to_owned(),
    ))
    .await
    .unwrap();
    let tool = app
        .state
        .tools
        .create(AgentToolName::Codex, Some("/explicit/codex".into()))
        .await
        .unwrap();
    events.try_recv().unwrap();
    for verb in ["UPDATE", "DELETE"] {
        db.execute(Statement::from_string(DbBackend::Sqlite,format!("CREATE TRIGGER reject_tool_mutation BEFORE {verb} ON agent_tools BEGIN SELECT RAISE(ABORT,'injected tool mutation failure'); END"))).await.unwrap();
        let failed = if verb == "UPDATE" {
            app.state.tools.update(tool.id, None).await.is_err()
        } else {
            app.state.tools.delete(tool.id).await.is_err()
        };
        assert_that!(&failed).is_true();
        let current = app.state.tools.list().await.unwrap().remove(0);
        assert_that!(&current.executable_path.as_deref()).is_equal_to(Some("/explicit/codex"));
        assert_that!(&events.try_recv().is_err()).is_true();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_tool_mutation".to_owned(),
        ))
        .await
        .unwrap();
    }
    assert_that!(
        &app.state
            .tools
            .update(tool.id, Some(String::new()))
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state.tools.delete(tool.id).await.unwrap();
    events.try_recv().unwrap();
    assert_that!(&events.try_recv().is_err()).is_true();
}
