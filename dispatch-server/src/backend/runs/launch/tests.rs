use assertr::prelude::*;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use tempfile::TempDir;

use super::policy::domain_sha256;
use super::{model::*, repository::*};
use crate::backend::execution::identity as agent_ids;
use crate::backend::storage::Store;
use dispatch_types::AgentRunPurposeV1;
use rootcause::Result;

#[test]
fn launch_target_canonical_bytes_are_strict() {
    let target = AgentLaunchTargetV1::specific(198, 7).unwrap();
    let encoded = target.encode().unwrap();
    assert_that!(&encoded.json).is_equal_to(
        "{\"kind\":\"specific\",\"schema_version\":1,\"work_item_id\":198,\"expected_version\":7}",
    );
    assert_that!(&(AgentLaunchTargetV1::decode(&encoded.json, &encoded.sha256).unwrap()))
        .is_equal_to(target);
    let noncanonical =
        "{ \"kind\":\"specific\",\"schema_version\":1,\"work_item_id\":198,\"expected_version\":7}";
    let digest = domain_sha256("dispatch.agent-launch-target.v1", noncanonical.as_bytes());
    assert_that!(&(AgentLaunchTargetV1::decode(noncanonical, &digest).is_err())).is_true();
    let unknown = "{\"kind\":\"none\",\"schema_version\":1,\"extra\":true}";
    let digest = domain_sha256("dispatch.agent-launch-target.v1", unknown.as_bytes());
    assert_that!(&(AgentLaunchTargetV1::decode(unknown, &digest).is_err())).is_true();
}

#[test]
fn all_four_launch_targets_are_closed_and_distinct() {
    let targets = [
        AgentLaunchTargetV1::none(),
        AgentLaunchTargetV1::next_open("open").unwrap(),
        AgentLaunchTargetV1::selector(&crudkit_core::condition::Condition::all()).unwrap(),
        AgentLaunchTargetV1::specific(198, 1).unwrap(),
    ];
    let records = targets
        .iter()
        .map(AgentLaunchTargetV1::encode)
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_that!(&records[0].json).contains("\"kind\":\"none\"");
    assert_that!(&records[1].json).contains("\"kind\":\"next_open\"");
    assert_that!(&records[2].json).contains("\"kind\":\"selector\"");
    assert_that!(&records[3].json).contains("\"kind\":\"specific\"");
    assert_that!(
        &(records
            .iter()
            .map(|record| &record.sha256)
            .collect::<std::collections::BTreeSet<_>>()
            .len())
    )
    .is_equal_to(4);
}

#[tokio::test]
async fn direct_none_target_never_claims_item_198() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO projects (id, name, display_name) VALUES (1, 'demo', 'Demo')".to_owned(),
        ))
        .await
        .unwrap();
    store.db().execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO work_items (id, project_id, title, description, version) VALUES (198, 1, 'Do not claim', '', 1)".to_owned(),
        )).await.unwrap();
    let txn = store.db().begin().await.unwrap();
    txn.execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO agent_runs (id, project_id, run_kind, purpose, tool_name, mutability, status, command, working_dir) VALUES (1, 1, 'task', 'ordinary', 'codex', 'mutating', 'running', '', '')".to_owned(),
        )).await.unwrap();
    insert_contract_in_tx(
        &txn,
        1,
        1,
        AgentRunPurposeV1::Ordinary,
        &AgentLaunchTargetV1::none(),
        &AgentCapabilitySetV1::ordinary(),
        "2026-09-05T12:00:00Z",
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let resolved = crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .resolve_agent_run_target(
            "demo",
            1,
            &agent_ids::dispatch_run_agent_id(1),
            &AgentLaunchTargetV1::none(),
            None,
        )
        .await
        .unwrap();
    let item = store
        .db()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT claimed_by, version FROM work_items WHERE id = 198".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    let contract = AgentRunLaunchRepository
        .load_in(
            &crate::backend::storage::TransactionManager::new(&store)
                .begin()
                .await
                .unwrap(),
            1,
            1,
        )
        .await
        .unwrap()
        .unwrap();
    assert_that!(&resolved).is_none();
    assert_that!(&item.try_get::<Option<String>>("", "claimed_by").unwrap()).is_none();
    assert_that!(&item.try_get::<i64>("", "version").unwrap()).is_equal_to(1);
    assert_that!(&contract.resolution).is_equal_to(AgentLaunchResolutionV1::none());
}
