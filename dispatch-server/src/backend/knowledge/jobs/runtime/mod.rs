//! Sequential passes share a durable active-work budget and resume valid checkpoints.
pub(crate) mod processes;
use super::*;
use crate::backend::{
    automation::knowledge_execution,
    entities::{
        agent_run::{self, AgentRun},
        project::Project,
    },
    process_sessions::ProcessSessionStart,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::{Mutex, watch};

pub(crate) fn spawn_until(
    store: Store,
    sessions: ProcessSessionRegistry,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let active = Arc::new(Mutex::new(HashSet::new()));
        loop {
            if *shutdown.borrow() {
                break;
            }
            let rows = store
                .db()
                .query_all(statement(
                    "SELECT id,project_id FROM knowledge_jobs WHERE unresolved=1",
                    vec![],
                ))
                .await;
            if let Ok(rows) = rows {
                for row in rows {
                    let Ok(id) = row.try_get::<i64>("", "id") else {
                        continue;
                    };
                    let Ok(project_id) = row.try_get::<i64>("", "project_id") else {
                        continue;
                    };
                    let Ok(record) = load(&store, project_id, id).await else {
                        continue;
                    };
                    if !matches!(record.job().status, JobStatus::Queued | JobStatus::Running)
                        || !active.lock().await.insert(id)
                    {
                        continue;
                    }
                    let (store, sessions, shutdown, active) = (
                        store.clone(),
                        sessions.clone(),
                        shutdown.clone(),
                        active.clone(),
                    );
                    tokio::spawn(async move {
                        if let Err(e) = drive(&store, &sessions, shutdown, project_id, id).await {
                            tracing::warn!(job_id = id, "Knowledge execution stopped: {e}");
                            let _lock = store.lock_knowledge_jobs().await;
                            if let Ok(mut record) = load(&store, project_id, id).await {
                                let _ = processes::recover(&record).await;
                                if let Some(run) = record.job().active_run_id {
                                    let _ = knowledge_execution::finish_interrupted(
                                        &store,
                                        run,
                                        &e.to_string(),
                                    )
                                    .await;
                                    sessions.finish(run);
                                    record.job_mut().active_run_id = None;
                                }
                                if !matches!(
                                    record.job().status,
                                    JobStatus::AwaitingReview
                                        | JobStatus::Cancelled
                                        | JobStatus::Completed
                                ) {
                                    record.job_mut().status = JobStatus::Failed;
                                }
                                record.job_mut().outcome = e.to_string();
                                let _ = persist(&store, &mut record).await;
                            }
                        }
                        active.lock().await.remove(&id);
                    });
                }
            }
            tokio::select! { _=tokio::time::sleep(Duration::from_secs(2))=>{},_=shutdown.changed()=>{} }
        }
    });
}

pub(crate) async fn recover(store: &Store) -> Result<()> {
    let _admission = store.lock_runtime_admission().await;
    let _lock = store.lock_knowledge_jobs().await;
    let rows = store
        .db()
        .query_all(statement(
            "SELECT id,project_id FROM knowledge_jobs WHERE unresolved=1",
            vec![],
        ))
        .await?;
    for row in rows {
        let mut record = load(
            store,
            row.try_get("", "project_id")?,
            row.try_get("", "id")?,
        )
        .await?;
        let linked = AgentRun::find()
            .filter(agent_run::Column::KnowledgeJobId.eq(record.job().id))
            .filter(agent_run::Column::Status.eq("running"))
            .all(store.db().as_ref())
            .await?;
        for run in linked {
            if !record.job().run_ids.contains(&run.id) {
                record.job_mut().run_ids.push(run.id);
            }
            if record.job().active_run_id.is_none() {
                record.job_mut().active_run_id = Some(run.id);
            }
            knowledge_execution::finish_interrupted(
                store,
                run.id,
                "Server interrupted knowledge execution",
            )
            .await?;
        }
        processes::recover(&record).await?;
        if record.job().stage == JobStage::Publication {
            publication::recover(store, &mut record).await?;
            continue;
        }
        if let Some(run) = record.job().active_run_id {
            knowledge_execution::finish_interrupted(
                store,
                run,
                "Server interrupted this knowledge pass; checkpoint retained",
            )
            .await?;
            record.job_mut().active_run_id = None;
            // A checkpoint's persisted heartbeat is the charged active time; downtime is not charged.
            record.pass_started_at = None;
            if record.job().cancel_requested {
                record.job_mut().status = JobStatus::Cancelled;
                record.job_mut().outcome = "Cancelled; checkpoint retained".into();
            } else if record.job().recovery_attempts < 3
                && record.job().active_millis < record.job().request.budget_seconds * 1000
            {
                record.job_mut().recovery_attempts += 1;
                record.job_mut().status = JobStatus::Queued;
                record.job_mut().progress = "Resuming interrupted discovery from checkpoint".into();
            } else {
                record.job_mut().status = JobStatus::Failed;
                record.job_mut().outcome =
                    "Recovery limit or active-work budget exhausted; Retry authorizes another run"
                        .into();
            }
            persist(store, &mut record).await?;
        }
    }
    Ok(())
}

async fn prepare(store: &Store, project_id: i64, id: i64) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let mut record = load(store, project_id, id).await?;
    if record.job().stage != JobStage::Inventory
        || record.job().cancel_requested
        || !matches!(record.job().status, JobStatus::Queued | JobStatus::Running)
    {
        return Ok(());
    }
    let copy = record.clone();
    let files = tokio::task::spawn_blocking(move || {
        let inputs =
            Path::new(&copy.artifact_dir).join(format!("capture-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&inputs)?;
        let result = sources::capture(Path::new(&copy.workspace), &copy.directory, &inputs);
        match result {
            Ok(files) => {
                if copy.inputs().exists() {
                    fs::remove_dir_all(copy.inputs())?;
                }
                fs::rename(inputs, copy.inputs())?;
                Ok(files)
            }
            Err(error) => {
                let _ = fs::remove_dir_all(inputs);
                Err(error)
            }
        }
    })
    .await??;
    record.files = files;
    let omissions: Vec<String> = serde_json::from_slice(&fs::read(
        Path::new(&record.artifact_dir).join("source-omissions.json"),
    )?)?;
    if !omissions.is_empty() {
        record
            .detail
            .findings
            .retain(|f| f.id != "source-input-omissions");
        record.detail.findings.push(KnowledgeFinding {
            id: "source-input-omissions".into(),
            explanation: "Source input excludes binary, non-UTF-8, or oversized files; assess whether these omissions leave consequential gaps".into(),
            references: omissions,
            consequential: true,
        });
    }
    for file in &record.files {
        if let Some(relative) = file.path.strip_prefix(&format!("{}/", record.directory)) {
            let target = record.draft().join(&record.directory).join(relative);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(record.inputs().join(&file.path), target)?;
        }
    }
    if let Some(previous_id) = record.job().request.previous_job_id {
        let previous = load(store, project_id, previous_id).await?;
        if matches!(
            previous.job().status,
            JobStatus::Failed | JobStatus::Cancelled
        ) {
            let index = crate::backend::knowledge::documents::Index::read(
                &previous.draft(),
                &previous.directory,
            )?;
            for document in index.documents.values() {
                let relative = format!("{}/{}", record.directory, document.summary.path);
                let before = fs::read(previous.inputs().join(&relative)).ok();
                let current = fs::read(record.inputs().join(&relative)).ok();
                if before == current {
                    let target = record.draft().join(relative);
                    fs::create_dir_all(target.parent().unwrap())?;
                    fs::write(target, &document.markdown)?;
                } else {
                    record.detail.remaining.push(format!(
                        "Retained draft for '{}' has changed evidence; reconsider it",
                        document.summary.path
                    ));
                }
            }
        }
    }
    fs::create_dir_all(record.draft().join(&record.directory))?;
    record.job_mut().stage = JobStage::Discovery;
    record.job_mut().progress =
        "Inventory captured; discovering responsibilities and aspects".into();
    persist(store, &mut record).await
}

fn pass_seconds(record: &Record) -> u64 {
    let remaining = record
        .job()
        .request
        .budget_seconds
        .saturating_sub(record.job().active_millis.div_ceil(1000));
    match record.job().stage {
        JobStage::Discovery => (record.job().request.budget_seconds * 3 / 4)
            .saturating_sub(record.job().active_millis.div_ceil(1000))
            .min(600),
        JobStage::Synthesis => remaining / 2,
        JobStage::Reader => remaining / 2,
        _ => remaining,
    }
    .max(1)
}
#[async_trait::async_trait]
pub(super) trait PassAgent: Send + Sync {
    async fn execute(
        &self,
        store: &Store,
        sessions: ProcessSessionRegistry,
        shutdown: watch::Receiver<bool>,
        input: knowledge_execution::PassInput,
    ) -> Result<()>;
}
pub(super) struct RealAgent;
#[async_trait::async_trait]
impl PassAgent for RealAgent {
    async fn execute(
        &self,
        store: &Store,
        sessions: ProcessSessionRegistry,
        shutdown: watch::Receiver<bool>,
        input: knowledge_execution::PassInput,
    ) -> Result<()> {
        knowledge_execution::execute(store, sessions, shutdown, input).await
    }
}
async fn drive(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    shutdown: watch::Receiver<bool>,
    project_id: i64,
    id: i64,
) -> Result<()> {
    drive_with_agent(store, sessions, shutdown, project_id, id, &RealAgent).await
}
pub(super) async fn drive_with_agent(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    mut shutdown: watch::Receiver<bool>,
    project_id: i64,
    id: i64,
    agent: &dyn PassAgent,
) -> Result<()> {
    loop {
        match prepare(store, project_id, id).await {
            Ok(()) => break,
            Err(error) => {
                let _lock = store.lock_knowledge_jobs().await;
                let mut record = load(store, project_id, id).await?;
                if record.job().cancel_requested || *shutdown.borrow() {
                    return Ok(());
                }
                if record.job().recovery_attempts >= 3 {
                    return Err(error);
                }
                record.job_mut().recovery_attempts += 1;
                record.job_mut().progress = format!("Retrying interrupted input capture: {error}");
                persist(store, &mut record).await?;
            }
        }
    }
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        let record = load(store, project_id, id).await?;
        if !matches!(record.job().status, JobStatus::Running | JobStatus::Queued) {
            return Ok(());
        }
        let project = Project::find_by_id(project_id)
            .one(store.db().as_ref())
            .await?
            .ok_or_else(|| report!("project deleted"))?;
        if record.job().cancel_requested {
            let _lock = store.lock_knowledge_jobs().await;
            let mut r = load(store, project_id, id).await?;
            r.job_mut().status = JobStatus::Cancelled;
            persist(store, &mut r).await?;
            return Ok(());
        }
        if record.job().recovery_attempts > 3
            || record.job().active_millis >= record.job().request.budget_seconds * 1000
        {
            return finalize(store, project_id, id).await;
        }
        let settings = projects::get_settings(store, &project.name).await?;
        let counts =
            crate::backend::automation_admission::running_counts_for_project_id(store, project_id)
                .await?;
        if counts.read_only >= settings.max_read_only_agents {
            tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=shutdown.changed()=>{}}
            continue;
        }
        let timeout = pass_seconds(&record);
        let mut run = knowledge_execution::allocate(store, &project.name, id, timeout).await?;
        let registration = sessions.begin(ProcessSessionStart {
            run_id: run.id,
            project_id,
            project_name: project.name.clone(),
            tool_name: "codex".into(),
            command: "Knowledge discovery pass".into(),
            working_dir: record.draft().to_string_lossy().into_owned(),
        });
        if registration.cancellation_requested() {
            knowledge_execution::finish_interrupted(store, run.id, "Project admission closed")
                .await?;
            return Ok(());
        }
        let input = {
            let _lock = store.lock_knowledge_jobs().await;
            let mut record = load(store, project_id, id).await?;
            if record.job().cancel_requested {
                sessions.finish(run.id);
                knowledge_execution::finish_interrupted(store, run.id, "Cancelled before launch")
                    .await?;
                return Ok(());
            }
            record.job_mut().active_run_id = Some(run.id);
            record.job_mut().run_ids.push(run.id);
            record.job_mut().status = JobStatus::Running;
            record.pass_started_at = Some(timestamp_millis());
            let working = if matches!(record.job().stage, JobStage::Reader | JobStage::Review) {
                let working = Path::new(&record.artifact_dir).join(format!("reader-{}", run.id));
                // Only canonical draft documents, never extraction notes or expected answers.
                let inventory = crate::backend::knowledge::discovery::Discovery::read(
                    &record.draft(),
                    &record.directory,
                );
                for path in inventory.files {
                    let relative = path.strip_prefix(record.draft())?;
                    let target = working.join(relative);
                    fs::create_dir_all(target.parent().unwrap())?;
                    fs::copy(path, target)?;
                }
                fs::create_dir_all(&working)?;
                working
            } else {
                record.draft()
            };
            let prompt = prompt(&record)?;
            persist(store, &mut record).await?;
            knowledge_execution::PassInput {
                job_id: id,
                run: run.clone(),
                project: project.name.clone(),
                working,
                instructions: include_str!("instructions.md").into(),
                prompt,
                timeout_seconds: timeout,
                writable: matches!(
                    record.job().stage,
                    JobStage::Discovery | JobStage::Synthesis
                ),
            }
        };
        let execution = agent.execute(store, sessions.clone(), shutdown.clone(), input);
        tokio::pin!(execution);
        let result = loop {
            tokio::select! {
                result=&mut execution=>break result,
                _=tokio::time::sleep(Duration::from_secs(1))=>{
                    heartbeat(store,project_id,id).await?;
                    if load(store,project_id,id).await?.job().cancel_requested {sessions.cancel_run(&project.name,run.id);}
                }
            }
        };
        sessions.finish(run.id);
        let _lock = store.lock_knowledge_jobs().await;
        let mut record = load(store, project_id, id).await?;
        charge(&mut record);
        record.pass_started_at = None;
        record.job_mut().active_run_id = None;
        if let Some(current) = AgentRun::find_by_id(run.id)
            .one(store.db().as_ref())
            .await?
        {
            run = current;
        }
        if result.is_err() {
            knowledge_execution::finish_interrupted(
                store,
                run.id,
                &result.as_ref().err().unwrap().to_string(),
            )
            .await?;
        }
        if record.job().cancel_requested {
            record.job_mut().status = JobStatus::Cancelled;
            record.job_mut().outcome = "Cancelled; drafts and evidence retained".into();
            persist(store, &mut record).await?;
            return Ok(());
        }
        if *shutdown.borrow() {
            record.job_mut().status = JobStatus::Queued;
            record.job_mut().recovery_attempts += 1;
            record.job_mut().progress =
                "Interrupted by server shutdown; checkpoint retained".into();
            persist(store, &mut record).await?;
            return Ok(());
        }
        let reported = record.reports.iter().any(|(r, _)| *r == run.id);
        if !reported {
            // Preserve the synthesis/validation reserve even when a discovery pass times out.
            if record.job().stage == JobStage::Discovery
                && record.job().active_millis >= record.job().request.budget_seconds * 750
                && record.job().active_millis < record.job().request.budget_seconds * 1000
            {
                record.detail.remaining.push("Discovery pass ended without a final report; synthesize retained checkpoints as partial knowledge".into());
                record.job_mut().stage = JobStage::Synthesis;
                persist(store, &mut record).await?;
                continue;
            }
            if record.job().recovery_attempts < 3
                && record.job().active_millis < record.job().request.budget_seconds * 1000
            {
                record.job_mut().recovery_attempts += 1;
                record.job_mut().progress = format!(
                    "Recovering pass: {}",
                    result
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Agent exited without a report".into())
                );
                persist(store, &mut record).await?;
                continue;
            }
            record
                .detail
                .remaining
                .push("Pass did not submit a valid report before its execution limit".into());
            persist(store, &mut record).await?;
            drop(_lock);
            return finalize(store, project_id, id).await;
        }
        let total_tokens = AgentRun::find()
            .filter(agent_run::Column::KnowledgeJobId.eq(id))
            .all(store.db().as_ref())
            .await?
            .iter()
            .map(|r| {
                r.input_tokens
                    .unwrap_or(0)
                    .saturating_add(r.output_tokens.unwrap_or(0))
                    .max(0) as u64
            })
            .sum::<u64>();
        if record
            .job()
            .request
            .token_budget
            .is_some_and(|budget| total_tokens >= budget)
        {
            record
                .detail
                .remaining
                .push("Reported-token budget reached; in-flight turns may exceed it".into());
            persist(store, &mut record).await?;
            drop(_lock);
            return finalize(store, project_id, id).await;
        }
        match record.job().stage {
            JobStage::Discovery => {
                if record
                    .reports
                    .last()
                    .is_some_and(|(_, r)| r.ready_for_synthesis)
                    || record.job().active_millis >= record.job().request.budget_seconds * 750
                {
                    record.job_mut().stage = JobStage::Synthesis;
                }
            }
            JobStage::Synthesis => {
                evaluation::freeze_questions(&mut record)?;
                record.job_mut().stage = JobStage::Reader;
            }
            JobStage::Reader => record.job_mut().stage = JobStage::Review,
            JobStage::Review => {
                persist(store, &mut record).await?;
                drop(_lock);
                return finalize(store, project_id, id).await;
            }
            _ => {}
        }
        persist(store, &mut record).await?;
    }
}
fn charge(record: &mut Record) {
    if let Some(start) = record.pass_started_at {
        let now = timestamp_millis();
        record.job_mut().active_millis = record
            .job()
            .active_millis
            .saturating_add(now.saturating_sub(start).max(0) as u64);
        record.pass_started_at = Some(now);
    }
}
async fn heartbeat(store: &Store, project_id: i64, id: i64) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let mut record = load(store, project_id, id).await?;
    charge(&mut record);
    persist(store, &mut record).await
}
async fn finalize(store: &Store, project_id: i64, id: i64) -> Result<()> {
    let _lock = store.lock_knowledge_jobs().await;
    let mut record = load(store, project_id, id).await?;
    if record.job().cancel_requested {
        record.job_mut().status = JobStatus::Cancelled;
        record.job_mut().outcome = "Cancelled; drafts retained".into();
        persist(store, &mut record).await?;
        return Ok(());
    }
    if record.reports.is_empty() {
        record.job_mut().status = JobStatus::Failed;
        record.job_mut().outcome = format!(
            "Discovery failed without a valid checkpoint: {}",
            record.job().progress
        );
        persist(store, &mut record).await?;
        return Ok(());
    }
    let copy = record.clone();
    record = tokio::task::spawn_blocking(move || {
        let mut copy = copy;
        publication::candidate(&mut copy)?;
        Ok::<_, rootcause::Report>(copy)
    })
    .await??;
    let issues = evaluation::quality_issues(&record);
    let automatic = record.detail.validation.is_empty()
        && issues.is_empty()
        && (record.job().application_mode == ApplicationMode::Automatic
            || (record.initial && record.detail.changes.iter().all(|c| c.before.is_none())));
    record.detail.validation.extend(issues);
    record.job_mut().status = if record.detail.changes.is_empty() {
        JobStatus::Completed
    } else {
        JobStatus::AwaitingReview
    };
    record.job_mut().outcome = if record.detail.changes.is_empty() {
        if record.complete {
            "No change needed"
        } else {
            "Incomplete discovery; evidence and remaining scope retained"
        }
    } else {
        "Candidate ready for review"
    }
    .into();
    persist(store, &mut record).await?;
    drop(_lock);
    if automatic
        && !record.detail.changes.is_empty()
        && let Err(e) = publication::apply(store, project_id, id).await
    {
        let _lock = store.lock_knowledge_jobs().await;
        let mut record = load(store, project_id, id).await?;
        record.job_mut().outcome = format!("Candidate retained for review: {e}");
        persist(store, &mut record).await?;
    }
    Ok(())
}
fn prompt(record: &Record) -> Result<String> {
    let stage = record.job().stage;
    let mut prompt = format!(
        "Knowledge job {}. Stage: {stage:?}. Knowledge directory: {}. This pass has {} seconds; submit a checkpoint before it expires.\nAdditional user context: {}\n",
        record.job().id,
        record.directory,
        pass_seconds(record),
        record.job().request.context
    );
    match stage {
        JobStage::Reader => {
            prompt.push_str("Independently answer these reading questions. Start at the root and navigate using the knowledge CLI. You receive no extraction notes or expected answers. Use source fallback only when knowledge is insufficient. Report summary and answers with this exact JSON shape: {\"summary\":\"...\",\"answers\":[{\"question_id\":\"...\",\"answer\":\"...\",\"references\":[\"README.md#section\"],\"missing\":false}]}. References are strings; missing is a boolean. Explicitly report missing knowledge.\n");
            prompt.push_str(&serde_json::to_string(
                &record
                    .questions
                    .iter()
                    .map(|q| (&q.id, &q.question))
                    .collect::<Vec<_>>(),
            )?);
        }
        JobStage::Review => {
            prompt.push_str("Evaluate the independent answers against the fixed evidence/requirements below. Independently inspect the candidate's changed sections, existing owners, related explanations, and affected summaries for duplicated contracts, misplaced detail, and missing owner links, even when every reading answer is correct. A link beside repeated detail does not resolve duplication. Identify document paths/sections and the owning explanation in review_issues; keep these issues separate from answer correctness. Also check meaningful refinement, unsupported claims, missing exceptions, code transcription, and all affected ancestors. Do not edit files. Report summary, evaluations [{question_id: string, correct: boolean, issues: string[]}], review_issues: string[], findings, and remaining: string[]. Failures require repair or explicit partial review. Compression is meaningful only alongside correctness/unanswered questions; never remove a consequential qualification to reduce reading cost.\n");
            prompt.push_str(&serde_json::to_string(&record.questions)?);
            prompt.push_str(&serde_json::to_string(&record.detail.quality)?);
        }
        _ => {
            prompt.push_str("Reconcile prior discoveries with current source. Before drafting derive representative questions with evidence and an ordinary/exception case. Report summary, areas [{id,responsibility,questions,evidence,aspects,owners,remaining}], questions [{id,question,requirements,evidence:[{path,fingerprint,range:{start,end},content}]}], findings [{id,explanation,references,consequential}], remaining, ready_for_synthesis, complete, reviewed_documents. Synthesis must connect every detail to all affected parents and root, and review dependents; report reviewed IDs including unchanged parents. Complete means no known material coverage gaps, not just process success.\n");
            prompt.push_str(&serde_json::to_string(&record.detail.areas)?);
            prompt.push_str(&serde_json::to_string(&record.detail.findings)?);
            prompt.push_str(&serde_json::to_string(&record.detail.remaining)?);
            prompt.push_str("\nExisting frozen evaluation questions (preserve):\n");
            prompt.push_str(&serde_json::to_string(&record.questions)?);
        }
    }
    Ok(prompt)
}
