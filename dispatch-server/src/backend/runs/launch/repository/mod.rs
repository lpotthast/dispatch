mod encoding;
use super::model::*;
use crate::backend::storage::Transaction;
use dispatch_types::{AgentRunKind, AgentRunPurposeV1};
use rootcause::{Result, prelude::*};
use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};
use std::str::FromStr;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) struct AgentRunLaunchRepository;

pub(crate) async fn mark_spawned_in_tx<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
    at: &str,
) -> Result<()> {
    transition_state(
        conn,
        project_id,
        run_id,
        &[AgentRunLaunchState::TargetResolved],
        AgentRunLaunchState::Spawned,
        at,
    )
    .await
}

pub(crate) async fn mark_terminal_in_tx<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
    at: &str,
) -> Result<()> {
    let contract = load_contract(conn, project_id, run_id).await?;

    if contract.is_some_and(|contract| contract.state == AgentRunLaunchState::Prepared) {
        return resolve_contract_in_tx(
            conn,
            project_id,
            run_id,
            &AgentLaunchResolutionV1::unavailable("run_terminated_before_target_resolution"),
            None,
            at,
        )
        .await;
    }
    transition_state(
        conn,
        project_id,
        run_id,
        &[
            AgentRunLaunchState::TargetResolved,
            AgentRunLaunchState::Spawned,
            AgentRunLaunchState::Terminal,
        ],
        AgentRunLaunchState::Terminal,
        at,
    )
    .await
}

pub(crate) async fn insert_contract_in_tx<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
    purpose: AgentRunPurposeV1,
    target: &AgentLaunchTargetV1,
    capabilities: &AgentCapabilitySetV1,
    at: &str,
) -> Result<()> {
    if (purpose != AgentRunPurposeV1::Ordinary
        && !matches!(target, AgentLaunchTargetV1::None { .. }))
        || capabilities != &AgentCapabilitySetV1::ordinary()
    {
        bail!("knowledge launches require an explicit no-item target");
    }
    let target = target.encode()?;
    let resolution = match serde_json::from_str::<AgentLaunchTargetV1>(&target.json)? {
        AgentLaunchTargetV1::None { .. } => AgentLaunchResolutionV1::none(),
        _ => AgentLaunchResolutionV1::pending(),
    };
    let state = if matches!(resolution, AgentLaunchResolutionV1::None { .. }) {
        AgentRunLaunchState::TargetResolved
    } else {
        AgentRunLaunchState::Prepared
    };
    let resolution = resolution.encode()?;
    let capabilities = capabilities.encode()?;
    conn.execute(statement(
        conn.get_database_backend(),
        "INSERT INTO agent_run_launch_contracts (run_id, project_id, purpose, state, target_schema_version, target_json, target_sha256, resolution_schema_version, resolution_json, resolution_sha256, capability_schema_version, capability_json, capability_sha256, created_at, updated_at) VALUES (?, ?, ?, ?, 1, ?, ?, 1, ?, ?, 1, ?, ?, ?, ?)",
        vec![
            run_id.into(), project_id.into(), purpose.as_storage().into(),
            state.as_storage().into(), target.json.into(), target.sha256.into(),
            resolution.json.into(), resolution.sha256.into(), capabilities.json.into(),
            capabilities.sha256.into(), at.into(), at.into(),
        ],
    ))
    .await
    .context("failed to insert agent run launch contract")?;

    Ok(())
}

pub(crate) async fn load_contract<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
) -> Result<Option<PersistedLaunchContract>> {
    let row = conn.query_one(statement(
        conn.get_database_backend(),
        "SELECT c.purpose, c.state, c.target_json, c.target_sha256, c.resolution_json, c.resolution_sha256, c.capability_json, c.capability_sha256, c.created_at, c.updated_at, r.purpose AS projected_purpose, r.run_kind, r.work_item_id FROM agent_run_launch_contracts c JOIN agent_runs r ON r.project_id = c.project_id AND r.id = c.run_id WHERE c.project_id = ? AND c.run_id = ?",
        vec![project_id.into(), run_id.into()],
    )).await.context("failed to load agent run launch contract")?;
    let Some(row) = row else {
        return Ok(None);
    };
    let purpose = AgentRunPurposeV1::from_str(&row.try_get::<String>("", "purpose")?)?;
    let state = parse_state(&row.try_get::<String>("", "state")?)?;
    let target_json = row.try_get::<String>("", "target_json")?;
    let target_sha = row.try_get::<String>("", "target_sha256")?;
    let resolution_json = row.try_get::<String>("", "resolution_json")?;
    let resolution_sha = row.try_get::<String>("", "resolution_sha256")?;
    let capability_json = row.try_get::<String>("", "capability_json")?;
    let capability_sha = row.try_get::<String>("", "capability_sha256")?;
    let created_at = row.try_get::<String>("", "created_at")?;
    let updated_at = row.try_get::<String>("", "updated_at")?;
    let projected_purpose = row.try_get::<Option<String>>("", "projected_purpose")?;
    let run_kind = row.try_get::<String>("", "run_kind")?;
    let work_item_id = row.try_get::<Option<i64>>("", "work_item_id")?;
    if projected_purpose.as_deref() != Some(purpose.as_storage()) {
        bail!("agent run purpose projection disagrees with launch contract");
    }
    let created = OffsetDateTime::parse(&created_at, &Rfc3339)
        .context("invalid launch contract creation timestamp")?;
    let updated = OffsetDateTime::parse(&updated_at, &Rfc3339)
        .context("invalid launch contract update timestamp")?;
    if updated < created {
        bail!("launch contract update timestamp precedes creation");
    }
    let target = AgentLaunchTargetV1::decode(&target_json, &target_sha)?;
    let resolution = AgentLaunchResolutionV1::decode(&resolution_json, &resolution_sha)?;
    let capabilities = AgentCapabilitySetV1::decode(&capability_json, &capability_sha)?;
    validate_contract_projection(state, &target, &resolution)?;
    let resolved_item_id = match &resolution {
        AgentLaunchResolutionV1::Claimed { work_item_id, .. } => Some(*work_item_id),
        _ => None,
    };
    if work_item_id != resolved_item_id {
        bail!("agent run work-item projection disagrees with launch resolution");
    }
    match purpose {
        AgentRunPurposeV1::Ordinary
            if run_kind != AgentRunKind::Task.as_storage()
                || capabilities != AgentCapabilitySetV1::ordinary() =>
        {
            bail!("ordinary run kind or capability projection is invalid")
        }
        AgentRunPurposeV1::KnowledgeAnswer
            if run_kind != AgentRunKind::KnowledgeAnswer.as_storage()
                || !matches!(&target, AgentLaunchTargetV1::None { .. })
                || capabilities != AgentCapabilitySetV1::ordinary() =>
        {
            bail!("knowledge-answer run kind or target projection is invalid")
        }
        AgentRunPurposeV1::KnowledgeCycle
            if run_kind != AgentRunKind::Task.as_storage()
                || !matches!(&target, AgentLaunchTargetV1::None { .. }) =>
        {
            bail!("knowledge-cycle run kind or target projection is invalid")
        }
        _ => {}
    }
    Ok(Some(PersistedLaunchContract {
        project_id,
        run_id,
        purpose,
        state,
        target,
        resolution,
        capabilities,
    }))
}

pub(crate) async fn resolve_contract_in_tx<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
    resolution: &AgentLaunchResolutionV1,
    work_item_id: Option<i64>,
    at: &str,
) -> Result<()> {
    let record = resolution.encode()?;
    let state = if matches!(resolution, AgentLaunchResolutionV1::Unavailable { .. }) {
        AgentRunLaunchState::Terminal
    } else {
        AgentRunLaunchState::TargetResolved
    };
    let result = conn.execute(statement(
        conn.get_database_backend(),
        "UPDATE agent_run_launch_contracts SET state = ?, resolution_json = ?, resolution_sha256 = ?, updated_at = ? WHERE project_id = ? AND run_id = ? AND state = 'prepared'",
        vec![state.as_storage().into(), record.json.into(), record.sha256.into(), at.into(), project_id.into(), run_id.into()],
    )).await.context("failed to resolve agent run launch target")?;
    if result.rows_affected() != 1 {
        bail!("agent run launch target was already resolved or is missing");
    }
    let result = conn.execute(statement(
        conn.get_database_backend(),
        "UPDATE agent_runs SET work_item_id = ?, updated_at = ? WHERE project_id = ? AND id = ? AND work_item_id IS NULL",
        vec![work_item_id.into(), at.into(), project_id.into(), run_id.into()],
    )).await.context("failed to project launch resolution onto agent run")?;
    if result.rows_affected() != 1 {
        bail!("agent run compatibility work-item projection disagrees with launch contract");
    }
    Ok(())
}

async fn transition_state<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    run_id: i64,
    from: &[AgentRunLaunchState],
    to: AgentRunLaunchState,
    at: &str,
) -> Result<()> {
    let Some(current) = load_contract(conn, project_id, run_id).await? else {
        return Ok(()); // A pre-049 legacy run has no contract.
    };
    if current.state == to {
        return Ok(());
    }
    if !from.contains(&current.state) {
        bail!(
            "illegal agent run launch transition from {} to {}",
            current.state.as_storage(),
            to.as_storage()
        );
    }
    let result = conn.execute(statement(conn.get_database_backend(),
        "UPDATE agent_run_launch_contracts SET state = ?, updated_at = ? WHERE project_id = ? AND run_id = ? AND state = ?",
        vec![to.as_storage().into(), at.into(), project_id.into(), run_id.into(), current.state.as_storage().into()],
    )).await.context("failed to transition agent run launch contract")?;
    if result.rows_affected() != 1 {
        bail!("stale agent run launch transition");
    }
    Ok(())
}

fn validate_contract_projection(
    state: AgentRunLaunchState,
    target: &AgentLaunchTargetV1,
    resolution: &AgentLaunchResolutionV1,
) -> Result<()> {
    match (state, target, resolution) {
        (AgentRunLaunchState::Prepared, AgentLaunchTargetV1::None { .. }, _) => {
            bail!("none target cannot remain pending")
        }
        (AgentRunLaunchState::Prepared, _, AgentLaunchResolutionV1::Pending { .. })
        | (
            AgentRunLaunchState::TargetResolved
            | AgentRunLaunchState::Spawned
            | AgentRunLaunchState::Terminal,
            AgentLaunchTargetV1::None { .. },
            AgentLaunchResolutionV1::None { .. },
        )
        | (
            AgentRunLaunchState::TargetResolved
            | AgentRunLaunchState::Spawned
            | AgentRunLaunchState::Terminal,
            _,
            AgentLaunchResolutionV1::Claimed { .. },
        )
        | (AgentRunLaunchState::Terminal, _, AgentLaunchResolutionV1::Unavailable { .. }) => Ok(()),
        _ => bail!("agent run launch state, target, and resolution disagree"),
    }
}

fn parse_state(value: &str) -> Result<AgentRunLaunchState> {
    match value {
        "allocated" => Ok(AgentRunLaunchState::Allocated),
        "prepared" => Ok(AgentRunLaunchState::Prepared),
        "target_resolved" => Ok(AgentRunLaunchState::TargetResolved),
        "spawned" => Ok(AgentRunLaunchState::Spawned),
        "terminal" => Ok(AgentRunLaunchState::Terminal),
        _ => bail!("unknown agent run launch state {value}"),
    }
}

fn statement(backend: DbBackend, sql: &str, values: Vec<Value>) -> Statement {
    let sql = if backend == DbBackend::Postgres {
        let mut index = 0;
        sql.chars()
            .map(|character| {
                if character == '?' {
                    index += 1;
                    format!("${index}")
                } else {
                    character.to_string()
                }
            })
            .collect::<String>()
    } else {
        sql.to_owned()
    };
    Statement::from_sql_and_values(backend, sql, values)
}

impl AgentRunLaunchRepository {
    pub(crate) async fn load_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<PersistedLaunchContract>> {
        load_contract(transaction.connection(), project_id, run_id).await
    }
    pub(crate) async fn resolve_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
        resolution: &AgentLaunchResolutionV1,
        claimed_item_id: Option<i64>,
        at: &str,
    ) -> Result<()> {
        resolve_contract_in_tx(
            transaction.connection(),
            project_id,
            run_id,
            resolution,
            claimed_item_id,
            at,
        )
        .await
    }
}
