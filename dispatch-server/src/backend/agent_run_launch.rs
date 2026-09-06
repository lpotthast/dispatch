//! Closed, persisted agent-run launch authority.
//!
//! A nullable compatibility `agent_runs.work_item_id` is an observation, never permission to
//! claim work. Every new run has one immutable canonical target and moves through this state
//! machine. Legacy rows deliberately have no contract.
use std::str::FromStr;

use crudkit_core::condition::Condition;
use dispatch_types::{AgentRunLaunchResolutionView, AgentRunLaunchTargetView, AgentRunPurposeV1};
use rootcause::{Result, prelude::*};
use sea_orm::{ConnectionTrait, DbBackend, Statement, Value};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::storage::Store;
use crate::shared::view_models::AgentRunKind;

const MAX_CANONICAL_BYTES: usize = 16_384;
const MAX_TEXT: usize = 512;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AgentLaunchTargetV1 {
    None {
        schema_version: u32,
    },
    NextOpen {
        schema_version: u32,
        state: String,
    },
    Selector {
        schema_version: u32,
        selector_sha256: String,
    },
    Specific {
        schema_version: u32,
        work_item_id: i64,
        expected_version: i64,
    },
}

impl AgentLaunchTargetV1 {
    pub(crate) const fn none() -> Self {
        Self::None { schema_version: 1 }
    }

    #[cfg(test)]
    pub(crate) fn next_open(state: impl Into<String>) -> Result<Self> {
        let value = Self::NextOpen {
            schema_version: 1,
            state: state.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn selector(condition: &Condition) -> Result<Self> {
        Ok(Self::Selector {
            schema_version: 1,
            selector_sha256: canonical_value_sha256(
                "dispatch.agent-launch-selector.v1",
                condition,
            )?,
        })
    }

    pub(crate) fn specific(work_item_id: i64, expected_version: i64) -> Result<Self> {
        let value = Self::Specific {
            schema_version: 1,
            work_item_id,
            expected_version,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate_selector(&self, condition: &Condition) -> Result<()> {
        let Self::Selector {
            selector_sha256, ..
        } = self
        else {
            bail!("launch target is not a selector");
        };
        let actual = canonical_value_sha256("dispatch.agent-launch-selector.v1", condition)?;
        if !constant_time_eq(selector_sha256.as_bytes(), actual.as_bytes()) {
            bail!("automation selector does not match the persisted launch target");
        }
        Ok(())
    }

    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-launch-target.v1", self)
    }

    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-launch-target.v1",
            bytes,
            digest,
            Self::validate,
        )
    }

    fn validate(&self) -> Result<()> {
        let schema_version = match self {
            Self::None { schema_version }
            | Self::NextOpen { schema_version, .. }
            | Self::Selector { schema_version, .. }
            | Self::Specific { schema_version, .. } => *schema_version,
        };
        if schema_version != 1 {
            bail!("agent launch target schema_version must be 1");
        }
        match self {
            Self::None { .. } => {}
            Self::NextOpen { state, .. } => validate_text("state", state)?,
            Self::Selector {
                selector_sha256, ..
            } => validate_sha256("selector_sha256", selector_sha256)?,
            Self::Specific {
                work_item_id,
                expected_version,
                ..
            } => {
                if *work_item_id <= 0 || *expected_version <= 0 {
                    bail!("specific launch target item id and expected version must be positive");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AgentLaunchResolutionV1 {
    Pending {
        schema_version: u32,
    },
    None {
        schema_version: u32,
    },
    Claimed {
        schema_version: u32,
        work_item_id: i64,
        claimed_version: i64,
    },
    Unavailable {
        schema_version: u32,
        reason: String,
    },
}

impl AgentLaunchResolutionV1 {
    pub(crate) const fn pending() -> Self {
        Self::Pending { schema_version: 1 }
    }

    pub(crate) const fn none() -> Self {
        Self::None { schema_version: 1 }
    }

    pub(crate) fn claimed(work_item_id: i64, claimed_version: i64) -> Self {
        Self::Claimed {
            schema_version: 1,
            work_item_id,
            claimed_version,
        }
    }

    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            schema_version: 1,
            reason: reason.into(),
        }
    }

    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-launch-resolution.v1", self)
    }

    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-launch-resolution.v1",
            bytes,
            digest,
            Self::validate,
        )
    }

    fn validate(&self) -> Result<()> {
        let schema_version = match self {
            Self::Pending { schema_version }
            | Self::None { schema_version }
            | Self::Claimed { schema_version, .. }
            | Self::Unavailable { schema_version, .. } => *schema_version,
        };
        if schema_version != 1 {
            bail!("agent launch resolution schema_version must be 1");
        }
        match self {
            Self::Claimed {
                work_item_id,
                claimed_version,
                ..
            } if *work_item_id <= 0 || *claimed_version <= 0 => {
                bail!("claimed launch resolution values must be positive")
            }
            Self::Unavailable { reason, .. } => validate_text("reason", reason)?,
            _ => {}
        }
        Ok(())
    }
}

impl AgentLaunchTargetV1 {
    pub(crate) fn view(&self) -> AgentRunLaunchTargetView {
        match self {
            Self::None { schema_version } => AgentRunLaunchTargetView::None {
                schema_version: *schema_version,
            },
            Self::NextOpen {
                schema_version,
                state,
            } => AgentRunLaunchTargetView::NextOpen {
                schema_version: *schema_version,
                state: state.clone(),
            },
            Self::Selector {
                schema_version,
                selector_sha256,
            } => AgentRunLaunchTargetView::Selector {
                schema_version: *schema_version,
                selector_sha256: selector_sha256.clone(),
            },
            Self::Specific {
                schema_version,
                work_item_id,
                expected_version,
            } => AgentRunLaunchTargetView::Specific {
                schema_version: *schema_version,
                work_item_id: *work_item_id,
                expected_version: *expected_version,
            },
        }
    }
}

impl AgentLaunchResolutionV1 {
    pub(crate) fn view(&self) -> AgentRunLaunchResolutionView {
        match self {
            Self::Pending { schema_version } => AgentRunLaunchResolutionView::Pending {
                schema_version: *schema_version,
            },
            Self::None { schema_version } => AgentRunLaunchResolutionView::None {
                schema_version: *schema_version,
            },
            Self::Claimed {
                schema_version,
                work_item_id,
                claimed_version,
            } => AgentRunLaunchResolutionView::Claimed {
                schema_version: *schema_version,
                work_item_id: *work_item_id,
                claimed_version: *claimed_version,
            },
            Self::Unavailable {
                schema_version,
                reason,
            } => AgentRunLaunchResolutionView::Unavailable {
                schema_version: *schema_version,
                reason: reason.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCapabilitySetV1 {
    pub schema_version: u32,
    pub capabilities: Vec<String>,
}

impl AgentCapabilitySetV1 {
    pub(crate) fn ordinary() -> Self {
        Self {
            schema_version: 1,
            capabilities: Vec::new(),
        }
    }

    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-capability-set.v1", self)
    }

    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-capability-set.v1",
            bytes,
            digest,
            Self::validate,
        )
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("agent capability set schema_version must be 1");
        }
        if self.capabilities.windows(2).any(|pair| pair[0] >= pair[1]) {
            bail!("agent capabilities must be strictly ordered and unique");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentRunLaunchState {
    Allocated,
    Prepared,
    TargetResolved,
    Spawned,
    Terminal,
}

impl AgentRunLaunchState {
    pub(crate) const fn as_storage(self) -> &'static str {
        match self {
            Self::Allocated => "allocated",
            Self::Prepared => "prepared",
            Self::TargetResolved => "target_resolved",
            Self::Spawned => "spawned",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedLaunchContract {
    pub project_id: i64,
    pub run_id: i64,
    pub purpose: AgentRunPurposeV1,
    pub state: AgentRunLaunchState,
    pub target: AgentLaunchTargetV1,
    pub resolution: AgentLaunchResolutionV1,
    pub capabilities: AgentCapabilitySetV1,
}

pub(crate) struct AgentRunLaunchRepository<'a> {
    store: &'a Store,
}

impl<'a> AgentRunLaunchRepository<'a> {
    pub(crate) const fn new(store: &'a Store) -> Self {
        Self { store }
    }

    pub(crate) async fn load(
        &self,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<PersistedLaunchContract>> {
        load_contract(self.store.db().as_ref(), project_id, run_id).await
    }
}

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

#[derive(Clone, Debug)]
pub(crate) struct CanonicalRecord {
    pub(crate) json: String,
    pub(crate) sha256: String,
}

fn canonical_record<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalRecord> {
    let json =
        serde_json::to_string(value).context("failed to encode canonical agent run record")?;
    if json.len() > MAX_CANONICAL_BYTES {
        bail!("canonical agent run record is too large");
    }
    Ok(CanonicalRecord {
        sha256: domain_sha256(domain, json.as_bytes()),
        json,
    })
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    domain: &str,
    bytes: &str,
    expected_sha256: &str,
    validate: impl FnOnce(&T) -> Result<()>,
) -> Result<T> {
    if bytes.len() > MAX_CANONICAL_BYTES {
        bail!("canonical agent run record is too large");
    }
    validate_sha256("canonical record digest", expected_sha256)?;
    let actual = domain_sha256(domain, bytes.as_bytes());
    if !constant_time_eq(actual.as_bytes(), expected_sha256.as_bytes()) {
        bail!("canonical agent run record digest mismatch");
    }
    let value: T = serde_json::from_str(bytes).context("invalid canonical agent run record")?;
    validate(&value)?;
    let encoded = serde_json::to_string(&value)?;
    if encoded.as_bytes() != bytes.as_bytes() {
        bail!("agent run record is not canonical JSON");
    }
    Ok(value)
}

fn canonical_value_sha256<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("failed to encode launch selector")?;
    Ok(domain_sha256(domain, &bytes))
}

fn domain_sha256(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn validate_sha256(name: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{name} must be lowercase SHA-256 hex");
    }
    Ok(())
}

fn validate_text(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_TEXT
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        bail!("{name} must be bounded, trimmed, non-control text");
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{agent_ids, item_claims};

    #[test]
    fn launch_target_canonical_bytes_are_strict() {
        let target = AgentLaunchTargetV1::specific(198, 7).unwrap();
        let encoded = target.encode().unwrap();
        assert_that!(&encoded.json).is_equal_to("{\"kind\":\"specific\",\"schema_version\":1,\"work_item_id\":198,\"expected_version\":7}");
        assert_that!(&(AgentLaunchTargetV1::decode(&encoded.json, &encoded.sha256).unwrap()))
            .is_equal_to(target);
        let noncanonical = "{ \"kind\":\"specific\",\"schema_version\":1,\"work_item_id\":198,\"expected_version\":7}";
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
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        store
            .db()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO projects (id, name, display_name) VALUES (1, 'demo', 'Demo')"
                    .to_owned(),
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

        let resolved = item_claims::resolve_agent_run_target(
            &store,
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
        let contract = AgentRunLaunchRepository::new(&store)
            .load(1, 1)
            .await
            .unwrap()
            .unwrap();
        assert_that!(&resolved).is_none();
        assert_that!(&item.try_get::<Option<String>>("", "claimed_by").unwrap()).is_none();
        assert_that!(&item.try_get::<i64>("", "version").unwrap()).is_equal_to(1);
        assert_that!(&contract.resolution).is_equal_to(AgentLaunchResolutionV1::none());
    }
}
