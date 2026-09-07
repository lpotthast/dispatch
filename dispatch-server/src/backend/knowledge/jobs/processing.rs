use super::{
    evaluation,
    execution::{PassAgent, PassInput},
    model::Record,
    policy::{charge, pass_seconds, prompt, timestamp_millis},
    service::JobService,
};
use crate::backend::{
    execution::sessions::ProcessSessionStart,
    runs::{launch::model::AgentLaunchTargetV1, model::CreateRunConfig},
};
use dispatch_types::{
    AgentRunKind, AgentRunPurposeV1, AgentRunStatus, AgentRunView, AgentToolName,
    AutomationExecutionPolicy, AutomationRunMutability, ProjectSettingsView, ProjectView,
    knowledge::jobs::*,
};
use rootcause::Result;
use std::time::Duration;
use tokio::sync::watch;
impl JobService {
    pub(super) async fn prepare(&self, project_id: i64, id: i64) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let mut record = self.load(project_id, id).await?;
        if record.job().stage != JobStage::Inventory
            || record.job().cancel_requested
            || !matches!(record.job().status, JobStatus::Queued | JobStatus::Running)
        {
            return Ok(());
        }
        let previous = match record.job().request.previous_job_id {
            Some(id) => Some(self.load(project_id, id).await?),
            None => None,
        };
        record = self.files.prepare_inputs(record, previous).await?;
        record.job_mut().stage = JobStage::Discovery;
        record.job_mut().progress =
            "Inventory captured; discovering responsibilities and aspects".into();
        self.persist(&mut record).await
    }
    pub(super) async fn begin_pass(
        &self,
        project_id: i64,
        id: i64,
        timeout: u64,
    ) -> Result<Option<(Record, AgentRunView, ProjectView, ProjectSettingsView)>> {
        let _admission = self.admission.acquire().await;
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let project = self.projects.by_id_in(&tx, project_id).await?;
        let settings = self.projects.settings_by_id_in(&tx, project_id).await?;
        let mut record = self.repository.load_in(&tx, project_id, id).await?;
        if record.job().cancel_requested
            || !matches!(record.job().status, JobStatus::Running | JobStatus::Queued)
        {
            tx.commit().await?;
            return Ok(None);
        }
        if self
            .admission
            .running_counts_in(&tx, project_id)
            .await?
            .read_only
            >= settings.max_read_only_agents
        {
            tx.commit().await?;
            return Ok(None);
        }
        self.admission
            .enforce_in(
                &tx,
                &project.name,
                &settings,
                AutomationRunMutability::ReadOnly,
                None,
                &AutomationExecutionPolicy::default(),
            )
            .await?;
        let run = self
            .runs
            .create_in(
                &tx,
                project_id,
                CreateRunConfig {
                    tool: AgentToolName::Codex,
                    mutability: AutomationRunMutability::ReadOnly,
                    trigger: None,
                    personality_revision_id: None,
                    effective_timeout_seconds: timeout,
                    effective_concurrency_group: None,
                    run_kind: AgentRunKind::Task,
                    purpose: AgentRunPurposeV1::KnowledgeCycle,
                    knowledge_job_id: Some(id),
                    launch_target: &AgentLaunchTargetV1::none(),
                },
            )
            .await?;
        record.job_mut().active_run_id = Some(run.id);
        record.job_mut().run_ids.push(run.id);
        record.job_mut().status = JobStatus::Running;
        record.pass_started_at = Some(timestamp_millis());
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await?;
        record.version += 1;
        self.events
            .publish_agent_run_changed(&project.name, run.id, None);
        Ok(Some((record, run, project, settings)))
    }
    pub(crate) async fn drive(
        &self,
        shutdown: watch::Receiver<bool>,
        project_id: i64,
        id: i64,
    ) -> Result<()> {
        self.drive_with_agent(shutdown, project_id, id, self.execution.as_ref())
            .await
    }
    pub(crate) async fn drive_with_agent(
        &self,
        mut shutdown: watch::Receiver<bool>,
        project_id: i64,
        id: i64,
        agent: &dyn PassAgent,
    ) -> Result<()> {
        self.prepare(project_id, id).await?;
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            let record = self.load(project_id, id).await?;
            if !matches!(record.job().status, JobStatus::Running | JobStatus::Queued) {
                return Ok(());
            }
            if record.job().cancel_requested {
                let _lock = self.coordination.lock().await;
                let mut record = self.load(project_id, id).await?;
                record.job_mut().status = JobStatus::Cancelled;
                self.persist(&mut record).await?;
                return Ok(());
            }
            if record.job().recovery_attempts > 3
                || record.job().active_millis >= record.job().request.budget_seconds * 1000
            {
                return self.finalize(project_id, id).await;
            }
            let timeout = pass_seconds(&record);
            let Some((record, mut run, project, settings)) =
                self.begin_pass(project_id, id, timeout).await?
            else {
                tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=shutdown.changed()=>{}}
                continue;
            };
            let registration = self.sessions.begin(ProcessSessionStart {
                run_id: run.id,
                project_id,
                project_name: project.name.clone(),
                tool_name: "codex".into(),
                command: "Knowledge discovery pass".into(),
                working_dir: record.draft().to_string_lossy().into_owned(),
            });
            if registration.cancellation_requested() {
                self.finish_interrupted(project_id, run.id, "Project admission closed")
                    .await?;
                return Ok(());
            }
            let working = self.files.pass_workspace(record.clone(), run.id).await?;
            let input = PassInput {
                job_id: id,
                run: run.clone(),
                project: project.name.clone(),
                working,
                instructions: include_str!("instructions.md").into(),
                prompt: prompt(&record)?,
                timeout_seconds: timeout,
                writable: matches!(
                    record.job().stage,
                    JobStage::Discovery | JobStage::Synthesis
                ),
                settings,
                artifact_dir: record.artifact_dir.into(),
            };
            let execution = agent.execute(shutdown.clone(), input);
            tokio::pin!(execution);
            let result = loop {
                tokio::select! {
                    result=&mut execution=>break result,
                    _=tokio::time::sleep(Duration::from_secs(1))=>{
                        self.heartbeat(project_id,id).await?;
                        if self.load(project_id,id).await?.job().cancel_requested {self.sessions.cancel_run(&project.name,run.id);}
                    }
                }
            };
            self.sessions.finish(run.id);
            let _lock = self.coordination.lock().await;
            let mut record = self.load(project_id, id).await?;
            charge(&mut record, timestamp_millis());
            record.pass_started_at = None;
            record.job_mut().active_run_id = None;
            if let Some(current) = self.runs.find_scoped(project_id, run.id).await? {
                run = current;
            }
            if result.is_err() {
                self.finish_interrupted(
                    project_id,
                    run.id,
                    &result.as_ref().err().unwrap().to_string(),
                )
                .await?;
            }
            if record.job().cancel_requested {
                record.job_mut().status = JobStatus::Cancelled;
                record.job_mut().outcome = "Cancelled; drafts and evidence retained".into();
                self.persist(&mut record).await?;
                return Ok(());
            }
            if *shutdown.borrow() {
                record.job_mut().status = JobStatus::Queued;
                record.job_mut().recovery_attempts += 1;
                record.job_mut().progress =
                    "Interrupted by server shutdown; checkpoint retained".into();
                self.persist(&mut record).await?;
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
                    self.persist(&mut record).await?;
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
                    self.persist(&mut record).await?;
                    continue;
                }
                record
                    .detail
                    .remaining
                    .push("Pass did not submit a valid report before its execution limit".into());
                self.persist(&mut record).await?;
                drop(_lock);
                return self.finalize(project_id, id).await;
            }
            let tx = self.transactions.begin().await?;
            let total_tokens = self.repository.run_tokens_in(&tx, project_id, id).await?;
            tx.commit().await?;
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
                self.persist(&mut record).await?;
                drop(_lock);
                return self.finalize(project_id, id).await;
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
                    self.persist(&mut record).await?;
                    drop(_lock);
                    return self.finalize(project_id, id).await;
                }
                _ => {}
            }
            self.persist(&mut record).await?;
        }
    }
    async fn heartbeat(&self, project_id: i64, id: i64) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let mut record = self.load(project_id, id).await?;
        charge(&mut record, timestamp_millis());
        self.persist(&mut record).await
    }
    async fn finalize(&self, project_id: i64, id: i64) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let mut record = self.load(project_id, id).await?;
        if record.job().cancel_requested {
            record.job_mut().status = JobStatus::Cancelled;
            record.job_mut().outcome = "Cancelled; drafts retained".into();
            self.persist(&mut record).await?;
            return Ok(());
        }
        if record.reports.is_empty() {
            record.job_mut().status = JobStatus::Failed;
            record.job_mut().outcome = format!(
                "Discovery failed without a valid checkpoint: {}",
                record.job().progress
            );
            self.persist(&mut record).await?;
            return Ok(());
        }
        record = self.files.candidate(record).await?;
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
        self.persist(&mut record).await?;
        drop(_lock);
        if automatic
            && !record.detail.changes.is_empty()
            && let Err(e) = self.apply(&self.projects.name(project_id).await?, id).await
        {
            let _lock = self.coordination.lock().await;
            let mut record = self.load(project_id, id).await?;
            record.job_mut().outcome = format!("Candidate retained for review: {e}");
            self.persist(&mut record).await?;
        }
        Ok(())
    }
    pub(crate) async fn recover(&self) -> Result<()> {
        let _admission = self.admission.acquire().await;
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let ids = self.repository.unresolved_in(&tx, None).await?;
        tx.commit().await?;
        for (project_id, id) in ids {
            let mut record = self.load(project_id, id).await?;
            let tx = self.transactions.begin().await?;
            let linked = self
                .repository
                .linked_running_runs_in(&tx, project_id, id)
                .await?;
            tx.commit().await?;
            for run in linked {
                if !record.job().run_ids.contains(&run) {
                    record.job_mut().run_ids.push(run);
                }
                if record.job().active_run_id.is_none() {
                    record.job_mut().active_run_id = Some(run);
                }
                self.finish_interrupted(project_id, run, "Server interrupted knowledge execution")
                    .await?;
            }
            self.files.recover_processes(&record).await?;
            if record.job().stage == JobStage::Publication {
                self.recover_publication(&mut record).await?;
                continue;
            }
            if let Some(run) = record.job().active_run_id {
                self.finish_interrupted(
                    project_id,
                    run,
                    "Server interrupted this knowledge pass; checkpoint retained",
                )
                .await?;
                record.job_mut().active_run_id = None;
                record.pass_started_at = None;
                if record.job().cancel_requested {
                    record.job_mut().status = JobStatus::Cancelled;
                    record.job_mut().outcome = "Cancelled; checkpoint retained".into();
                } else if record.job().recovery_attempts < 3
                    && record.job().active_millis < record.job().request.budget_seconds * 1000
                {
                    record.job_mut().recovery_attempts += 1;
                    record.job_mut().status = JobStatus::Queued;
                    record.job_mut().progress =
                        "Resuming interrupted discovery from checkpoint".into();
                } else {
                    record.job_mut().status = JobStatus::Failed;
                    record.job_mut().outcome="Recovery limit or active-work budget exhausted; Retry authorizes another run".into();
                }
                self.persist(&mut record).await?;
            }
        }
        Ok(())
    }
    pub(super) async fn finish_interrupted(
        &self,
        project_id: i64,
        run_id: i64,
        message: &str,
    ) -> Result<()> {
        if let Some(run) = self.runs.find_scoped(project_id, run_id).await?
            && run.status == AgentRunStatus::Running
        {
            self.runs
                .finish(run, AgentRunStatus::Failed, None, message.into())
                .await?;
        }
        Ok(())
    }
    pub(super) async fn unresolved(&self) -> Result<Vec<(i64, i64)>> {
        let tx = self.transactions.begin().await?;
        let ids = self.repository.unresolved_in(&tx, None).await?;
        tx.commit().await?;
        Ok(ids)
    }
    pub(super) async fn fail(&self, project_id: i64, id: i64, error: &str) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let mut record = self.load(project_id, id).await?;
        self.files.recover_processes(&record).await?;
        if let Some(run) = record.job().active_run_id {
            self.finish_interrupted(project_id, run, error).await?;
            self.sessions.finish(run);
            record.job_mut().active_run_id = None;
        }
        if !matches!(
            record.job().status,
            JobStatus::AwaitingReview | JobStatus::Cancelled | JobStatus::Completed
        ) {
            record.job_mut().status = JobStatus::Failed;
        }
        record.job_mut().outcome = error.to_owned();
        self.persist(&mut record).await
    }
}
