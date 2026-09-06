//! Bounded discovery with persisted admission, checkpoints and inspectable publication.
mod api;
mod sources;
pub(crate) use api::routes;
mod evaluation;
mod publication;
pub(crate) mod runtime;
#[cfg(test)]
mod tests;

use crate::backend::{
    process_sessions::ProcessSessionRegistry,
    projects,
    request_attribution::RequestAttribution,
    storage::{Store, utc_now},
};
use dispatch_types::knowledge::jobs::*;
use rootcause::{Result, prelude::*};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Supplied {
    run_id: i64,
    path: String,
    fingerprint: String,
    range: LineRange,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Record {
    detail: KnowledgeJobDetail,
    workspace: String,
    directory: String,
    artifact_dir: String,
    files: Vec<SourceFile>,
    supplied: Vec<Supplied>,
    questions: Vec<ReadingQuestion>,
    reports: Vec<(i64, JobReport)>,
    reviewed_documents: Vec<String>,
    initial: bool,
    complete: bool,
    version: i64,
    pass_started_at: Option<i64>,
}
impl Record {
    fn job(&self) -> &KnowledgeJob {
        self.detail.job.as_ref().expect("persisted job")
    }
    fn job_mut(&mut self) -> &mut KnowledgeJob {
        self.detail.job.as_mut().expect("persisted job")
    }
    fn draft(&self) -> PathBuf {
        Path::new(&self.artifact_dir).join("draft")
    }
    fn inputs(&self) -> PathBuf {
        Path::new(&self.artifact_dir).join("inputs")
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn estimated_tokens(bytes: &[u8]) -> u64 {
    bytes.len().div_ceil(4) as u64
}
fn timestamp_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
fn statement(sql: &str, values: Vec<Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
pub(crate) fn project_artifacts(store: &Store, project_id: i64) -> PathBuf {
    store
        .path()
        .parent()
        .unwrap()
        .join("knowledge-artifacts/projects")
        .join(project_id.to_string())
}
async fn load(store: &Store, project_id: i64, job_id: i64) -> Result<Record> {
    let row = store
        .db()
        .query_one(statement(
            "SELECT payload, version FROM knowledge_jobs WHERE project_id=? AND id=?",
            vec![project_id.into(), job_id.into()],
        ))
        .await?
        .ok_or_else(|| report!("knowledge job does not exist in this project"))?;
    let mut record: Record = serde_json::from_str(&row.try_get::<String>("", "payload")?)?;
    record.version = row.try_get("", "version")?;
    Ok(record)
}
async fn persist(store: &Store, record: &mut Record) -> Result<()> {
    record.job_mut().updated_at = utc_now();
    let result = store.db().execute(statement("UPDATE knowledge_jobs SET payload=?, unresolved=?, version=version+1 WHERE id=? AND project_id=? AND version=?",vec![serde_json::to_string(record)?.into(),record.job().status.unresolved().into(),record.job().id.into(),record.job().project_id.into(),record.version.into()])).await?;
    if result.rows_affected() != 1 {
        bail!("knowledge job changed concurrently; reload it");
    }
    record.version += 1;
    Ok(())
}

pub(crate) async fn start(
    store: &Store,
    project: &str,
    request: StartKnowledgeJob,
) -> Result<KnowledgeJob> {
    if request.request_id.trim().is_empty() || request.request_id.len() > 200 {
        bail!("start requires a stable request_id, at most 200 bytes");
    }
    if !(60..=86_400).contains(&request.budget_seconds)
        || request.token_budget == Some(0)
        || request.context.len() > 32_000
    {
        bail!(
            "budget must be 60–86400 seconds, token budget positive, and context at most 32000 bytes"
        );
    }
    let _lock = store.lock_knowledge_jobs().await;
    let project = projects::find_project_by_name(store, project).await?;
    if let Some(row) = store
        .db()
        .query_one(statement(
            "SELECT id FROM knowledge_jobs WHERE project_id=? AND request_id=?",
            vec![project.id.into(), request.request_id.clone().into()],
        ))
        .await?
    {
        let record = load(store, project.id, row.try_get("", "id")?).await?;
        if record.job().request != request {
            bail!("request_id is already used for different job settings");
        }
        return Ok(record.job().clone());
    }
    if store
        .db()
        .query_one(statement(
            "SELECT id FROM knowledge_jobs WHERE project_id=? AND unresolved=1",
            vec![project.id.into()],
        ))
        .await?
        .is_some()
    {
        bail!("resolve the project's existing knowledge job or proposal before starting another");
    }
    let defaults = settings_by_id(store, project.id).await?;
    let application_mode = request
        .application_mode
        .unwrap_or(defaults.application_mode);
    let workspace = project
        .path
        .ok_or_else(|| report!("project has no working directory"))?;
    let directory = super::normalize_knowledge_directory(&project.knowledge_directory)?;
    let previous = match request.previous_job_id {
        Some(id) => Some(load(store, project.id, id).await?),
        None => None,
    };
    let initial = !Path::new(&workspace)
        .join(&directory)
        .join("README.md")
        .exists();
    let now = utc_now();
    let job = KnowledgeJob {
        id: 0,
        project_id: project.id,
        request,
        application_mode,
        status: JobStatus::Queued,
        stage: JobStage::Inventory,
        progress: "Waiting for discovery".into(),
        current_area: None,
        current_aspect: None,
        active_millis: 0,
        recovery_attempts: 0,
        run_ids: vec![],
        active_run_id: None,
        cancel_requested: false,
        outcome: String::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let mut record = Record {
        detail: KnowledgeJobDetail {
            job: Some(job),
            ..Default::default()
        },
        workspace,
        directory,
        artifact_dir: String::new(),
        files: vec![],
        supplied: vec![],
        questions: vec![],
        reports: vec![],
        reviewed_documents: vec![],
        initial,
        complete: false,
        version: 0,
        pass_started_at: None,
    };
    if let Some(previous) = previous {
        record.detail.areas = previous.detail.areas;
        record.detail.assessments = previous.detail.assessments;
        record.detail.findings = previous.detail.findings;
        record.detail.remaining = previous.detail.remaining;
        record.questions = previous.questions;
        record.supplied = previous.supplied;
    }
    // UNIQUE(project_id, request_id) and the partial admission index also fence response loss/restarts.
    let transaction = store.db().begin().await?;
    let result = transaction
        .execute(statement(
            "INSERT INTO knowledge_jobs(project_id,request_id,payload) VALUES(?,?,?)",
            vec![
                project.id.into(),
                record.job().request.request_id.clone().into(),
                serde_json::to_string(&record)?.into(),
            ],
        ))
        .await?;
    let id = result.last_insert_id() as i64;
    record.job_mut().id = id;
    record.artifact_dir = project_artifacts(store, project.id)
        .join(format!("jobs/{id}"))
        .to_string_lossy()
        .into_owned();
    transaction
        .execute(statement(
            "UPDATE knowledge_jobs SET payload=? WHERE id=?",
            vec![serde_json::to_string(&record)?.into(), id.into()],
        ))
        .await?;
    transaction.commit().await?;
    Ok(record.job().clone())
}
pub(crate) async fn list(store: &Store, project: &str) -> Result<Vec<KnowledgeJob>> {
    let id = projects::project_id(store, project).await?;
    let rows = store
        .db()
        .query_all(statement(
            "SELECT json_extract(payload,'$.detail.job') AS job FROM knowledge_jobs WHERE project_id=? ORDER BY id DESC",
            vec![id.into()],
        ))
        .await?;
    rows.into_iter()
        .map(|row| Ok(serde_json::from_str(&row.try_get::<String>("", "job")?)?))
        .collect()
}
pub(crate) async fn detail(store: &Store, project: &str, id: i64) -> Result<KnowledgeJobDetail> {
    Ok(load(store, projects::project_id(store, project).await?, id)
        .await?
        .detail)
}
pub(crate) async fn coverage(
    store: &Store,
    project: &str,
    id: i64,
    aspect: Option<String>,
) -> Result<CoverageView> {
    let record = load(store, projects::project_id(store, project).await?, id).await?;
    tokio::task::spawn_blocking(move || sources::coverage(&record, aspect.as_deref())).await?
}
async fn active_record(
    store: &Store,
    project: &str,
    id: i64,
    attribution: &RequestAttribution,
) -> Result<(Record, i64)> {
    let run = attribution
        .agent_run_id
        .ok_or_else(|| report!("knowledge job operation requires active agent-run context"))?;
    let record = load(store, projects::project_id(store, project).await?, id).await?;
    if record.job().active_run_id != Some(run)
        || record.job().status != JobStatus::Running
        || record.job().cancel_requested
    {
        bail!("job/run context is no longer active or does not match");
    }
    Ok((record, run))
}
pub(crate) async fn source_query(
    store: &Store,
    project: &str,
    id: i64,
    attribution: &RequestAttribution,
    operation: String,
    query: SourceQuery,
) -> Result<SourceResponse> {
    let _lock = store.lock_knowledge_jobs().await;
    let (mut record, run) = active_record(store, project, id, attribution).await?;
    let (mut record, response) = tokio::task::spawn_blocking(move || -> Result<_> {
        let response = sources::query(&mut record, run, &operation, query)?;
        Ok((record, response))
    })
    .await??;
    persist(store, &mut record).await?;
    Ok(response)
}
pub(crate) async fn assessment(
    store: &Store,
    project: &str,
    id: i64,
    attribution: &RequestAttribution,
    assessment: AspectAssessment,
) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let (mut record, run) = active_record(store, project, id, attribution).await?;
    if record.job().stage != JobStage::Discovery {
        bail!("assessments belong to discovery passes");
    }
    sources::assess(&mut record, run, assessment)?;
    persist(store, &mut record).await
}
pub(crate) async fn progress(
    store: &Store,
    project: &str,
    id: i64,
    attribution: &RequestAttribution,
    progress: JobProgress,
) -> Result<()> {
    if progress.body.len() > 4000 {
        bail!("progress is limited to 4000 bytes");
    }
    let _lock = store.lock_knowledge_jobs().await;
    let (mut record, _) = active_record(store, project, id, attribution).await?;
    record.job_mut().progress = progress.body;
    record.job_mut().current_area = progress.area;
    record.job_mut().current_aspect = progress.aspect;
    persist(store, &mut record).await
}
pub(crate) async fn report(
    store: &Store,
    project: &str,
    id: i64,
    attribution: &RequestAttribution,
    report: JobReport,
) -> Result<()> {
    if serde_json::to_vec(&report)?.len() > 2 * 1024 * 1024 {
        bail!("job report exceeds 2 MiB");
    }
    let _lock = store.lock_knowledge_jobs().await;
    let (mut record, run) = active_record(store, project, id, attribution).await?;
    if let Some((_, old)) = record.reports.iter().find(|(r, _)| *r == run) {
        if *old == report {
            return Ok(());
        }
        bail!("this run has already submitted a different report");
    }
    evaluation::accept_report(&mut record, run, report)?;
    persist(store, &mut record).await
}
pub(crate) async fn record_read(store: &Store, run_id: i64, content: &[u8]) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let row=store.db().query_one(statement("SELECT project_id,knowledge_job_id FROM agent_runs WHERE id=? AND knowledge_job_id IS NOT NULL",vec![run_id.into()])).await?;
    if let Some(row) = row {
        let mut record = load(
            store,
            row.try_get("", "project_id")?,
            row.try_get("", "knowledge_job_id")?,
        )
        .await?;
        if record.job().stage == JobStage::Reader && record.job().active_run_id == Some(run_id) {
            record.detail.quality.reading_estimated_tokens += estimated_tokens(content);
            persist(store, &mut record).await?;
        }
    }
    Ok(())
}

pub(crate) async fn action(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    project: &str,
    id: i64,
    action: JobAction,
) -> Result<KnowledgeJob> {
    let project_id = projects::project_id(store, project).await?;
    if let JobAction::Retry { request_id } | JobAction::Continue { request_id } = &action {
        let record = load(store, project_id, id).await?;
        if record.job().status.unresolved() {
            bail!("resolve the existing job or pending proposal first");
        }
        let mut request = record.job().request.clone();
        request.request_id = request_id.clone();
        request.previous_job_id = Some(id);
        return start(store, project, request).await;
    }
    if action == JobAction::Apply {
        return publication::apply(store, project_id, id).await;
    }
    let _lock = store.lock_knowledge_jobs().await;
    let mut record = load(store, project_id, id).await?;
    match action {
        JobAction::Cancel => {
            if !record.job().status.unresolved() {
                return Ok(record.job().clone());
            }
            record.job_mut().cancel_requested = true;
            if record.job().active_run_id.is_none() && record.job().stage != JobStage::Publication {
                record.job_mut().status = JobStatus::Cancelled;
                record.job_mut().outcome = "Cancelled; drafts retained".into();
            }
            persist(store, &mut record).await?;
            if let Some(run) = record.job().active_run_id {
                sessions.cancel_run(project, run);
            }
        }
        JobAction::Reject => {
            if record.job().stage == JobStage::Publication {
                bail!("recover the interrupted publication before resolving this job");
            }
            if record.job().status != JobStatus::AwaitingReview {
                bail!("only a pending proposal can be rejected");
            }
            record.job_mut().status = JobStatus::Completed;
            record.job_mut().outcome = "Proposal rejected; draft retained".into();
            persist(store, &mut record).await?;
        }
        _ => unreachable!(),
    }
    Ok(record.job().clone())
}

/// Close persisted admission before lifecycle cleanup; queued jobs cannot recreate artifacts.
pub(crate) async fn stop_project(store: &Store, project_id: i64) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let rows = store
        .db()
        .query_all(statement(
            "SELECT id FROM knowledge_jobs WHERE project_id=? AND unresolved=1",
            vec![project_id.into()],
        ))
        .await?;
    for row in rows {
        let mut record = load(store, project_id, row.try_get("", "id")?).await?;
        record.job_mut().cancel_requested = true;
        record.job_mut().status = JobStatus::Cancelled;
        record.job_mut().outcome = "Project deletion stopped discovery".into();
        persist(store, &mut record).await?;
    }
    Ok(())
}

async fn settings_by_id(store: &Store, project_id: i64) -> Result<KnowledgeSettings> {
    match store
        .db()
        .query_one(statement(
            "SELECT payload FROM knowledge_job_settings WHERE project_id=?",
            vec![project_id.into()],
        ))
        .await?
    {
        Some(row) => Ok(serde_json::from_str(
            &row.try_get::<String>("", "payload")?,
        )?),
        None => Ok(KnowledgeSettings::default()),
    }
}
pub(crate) async fn settings(store: &Store, project: &str) -> Result<KnowledgeSettings> {
    settings_by_id(store, projects::project_id(store, project).await?).await
}
pub(crate) async fn save_settings(
    store: &Store,
    project: &str,
    settings: KnowledgeSettings,
) -> Result<KnowledgeSettings> {
    let project_id = projects::project_id(store, project).await?;
    store.db().execute(statement("INSERT INTO knowledge_job_settings(project_id,payload) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET payload=excluded.payload",vec![project_id.into(),serde_json::to_string(&settings)?.into()])).await?;
    Ok(settings)
}
