use super::{
    execution::KnowledgePassService, files::JobFiles, model::Record, policy::estimated_tokens,
    repository::JobRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    execution::sessions::ProcessSessionRegistry,
    projects::repository::ProjectRepository,
    runs::{admission::service::RunAdmissionService, service::RunService},
    storage::{Transaction, TransactionManager, utc_now},
};
use dispatch_types::knowledge::jobs::*;
use rootcause::{Result, prelude::*};
use std::sync::Arc;
use tokio::sync::Mutex;
pub(crate) struct JobService {
    pub(super) transactions: Arc<TransactionManager>,
    pub(super) projects: Arc<ProjectRepository>,
    pub(super) repository: Arc<JobRepository>,
    pub(super) files: Arc<JobFiles>,
    pub(super) runs: Arc<RunService>,
    pub(super) admission: Arc<RunAdmissionService>,
    pub(super) attribution: Arc<AttributionService>,
    pub(super) execution: Arc<KnowledgePassService>,
    pub(super) sessions: ProcessSessionRegistry,
    pub(super) events: UiEventBus,
    pub(super) coordination: Mutex<()>,
}
impl JobService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<JobRepository>,
        files: Arc<JobFiles>,
        runs: Arc<RunService>,
        admission: Arc<RunAdmissionService>,
        attribution: Arc<AttributionService>,
        execution: Arc<KnowledgePassService>,
        sessions: ProcessSessionRegistry,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            files,
            runs,
            admission,
            attribution,
            execution,
            sessions,
            events,
            coordination: Mutex::new(()),
        }
    }
    pub(crate) fn project_artifacts(&self, project_id: i64) -> std::path::PathBuf {
        self.files.project_artifacts(project_id)
    }
    pub(super) async fn load(&self, project_id: i64, id: i64) -> Result<Record> {
        let tx = self.transactions.begin().await?;
        self.projects.name_in(&tx, project_id).await?;
        let record = self.repository.load_in(&tx, project_id, id).await?;
        tx.commit().await?;
        Ok(record)
    }
    pub(super) async fn persist(&self, record: &mut Record) -> Result<()> {
        let tx = self.transactions.begin().await?;
        self.projects.name_in(&tx, record.job().project_id).await?;
        self.persist_in(&tx, record).await?;
        tx.commit().await?;
        record.version += 1;
        Ok(())
    }
    pub(super) async fn persist_in(&self, tx: &Transaction, record: &mut Record) -> Result<()> {
        record.job_mut().updated_at = utc_now();
        self.repository.persist_in(tx, record).await
    }
    async fn scoped_in(&self, tx: &Transaction, project: &str, id: i64) -> Result<Record> {
        let project_id = self.projects.id_in(tx, project).await?;
        self.repository.load_in(tx, project_id, id).await
    }
    pub(crate) async fn start(
        &self,
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
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let project = self.projects.by_name_in(&tx, project).await?;
        if let Some(record) = self
            .repository
            .idempotent_in(&tx, project.id, &request.request_id)
            .await?
        {
            if record.job().request != request {
                bail!("request_id is already used for different job settings");
            }
            tx.commit().await?;
            return Ok(record.job().clone());
        }
        if !self
            .repository
            .unresolved_in(&tx, Some(project.id))
            .await?
            .is_empty()
        {
            bail!(
                "resolve the project's existing knowledge job or proposal before starting another"
            );
        }
        let defaults = self.repository.settings_in(&tx, project.id).await?;
        tx.commit().await?;
        let workspace = project
            .path
            .clone()
            .ok_or_else(|| report!("project has no working directory"))?;
        let directory =
            super::super::policy::normalize_knowledge_directory(&project.knowledge_directory)?;
        let initial = self.files.root_is_absent(&workspace, &directory);
        let now = utc_now();
        let job = KnowledgeJob {
            id: 0,
            project_id: project.id,
            application_mode: request
                .application_mode
                .unwrap_or(defaults.application_mode),
            request,
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
        // File observations precede the short admission transaction; stored scope is revalidated inside it.
        let tx = self.transactions.begin().await?;
        let current = self.projects.by_id_in(&tx, project.id).await?;
        if current != project {
            bail!("project configuration changed during knowledge admission; retry the operation");
        }
        if let Some(existing) = self
            .repository
            .idempotent_in(&tx, project.id, &record.job().request.request_id)
            .await?
        {
            if existing.job().request != record.job().request {
                bail!("request_id is already used for different job settings");
            }
            tx.commit().await?;
            return Ok(existing.job().clone());
        }
        if !self
            .repository
            .unresolved_in(&tx, Some(project.id))
            .await?
            .is_empty()
        {
            bail!(
                "resolve the project's existing knowledge job or proposal before starting another"
            );
        }
        let current_defaults = self.repository.settings_in(&tx, project.id).await?;
        record.job_mut().application_mode = record
            .job()
            .request
            .application_mode
            .unwrap_or(current_defaults.application_mode);
        if let Some(previous_id) = record.job().request.previous_job_id {
            let previous = self
                .repository
                .load_in(&tx, project.id, previous_id)
                .await?;
            record.detail.areas = previous.detail.areas;
            record.detail.assessments = previous.detail.assessments;
            record.detail.findings = previous.detail.findings;
            record.detail.remaining = previous.detail.remaining;
            record.questions = previous.questions;
            record.supplied = previous.supplied;
        }
        let id = self.repository.allocate_in(&tx, &record).await?;
        record.job_mut().id = id;
        record.artifact_dir = self
            .files
            .project_artifacts(project.id)
            .join(format!("jobs/{id}"))
            .to_string_lossy()
            .into_owned();
        self.repository.initialize_in(&tx, &record).await?;
        tx.commit().await?;
        Ok(record.job().clone())
    }
    pub(crate) async fn list(&self, project: &str) -> Result<Vec<KnowledgeJob>> {
        let tx = self.transactions.begin().await?;
        let id = self.projects.id_in(&tx, project).await?;
        let jobs = self.repository.list_in(&tx, id).await?;
        tx.commit().await?;
        Ok(jobs)
    }
    pub(crate) async fn detail(&self, project: &str, id: i64) -> Result<KnowledgeJobDetail> {
        let tx = self.transactions.begin().await?;
        let record = self.scoped_in(&tx, project, id).await?;
        tx.commit().await?;
        Ok(record.detail)
    }
    pub(super) async fn active_record_in(
        &self,
        tx: &Transaction,
        project: &str,
        id: i64,
        input: AttributionInput,
    ) -> Result<(Record, i64)> {
        let attribution = self
            .attribution
            .validate_in(tx, project, input, true)
            .await?;
        let run = attribution
            .agent_run_id
            .ok_or_else(|| report!("knowledge job operation requires active agent-run context"))?;
        let record = self.scoped_in(tx, project, id).await?;
        if record.job().active_run_id != Some(run)
            || record.job().status != JobStatus::Running
            || record.job().cancel_requested
        {
            bail!("job/run context is no longer active or does not match");
        }
        Ok((record, run))
    }
    pub(crate) async fn coverage(
        &self,
        project: &str,
        id: i64,
        aspect: Option<String>,
        attribution: Option<AttributionInput>,
    ) -> Result<CoverageView> {
        let tx = self.transactions.begin().await?;
        let record = match attribution {
            Some(input) => {
                let (record, _) = self.active_record_in(&tx, project, id, input).await?;
                if record.job().stage == JobStage::Reader {
                    bail!("independent readers cannot inspect extraction assessments");
                }
                record
            }
            None => self.scoped_in(&tx, project, id).await?,
        };
        tx.commit().await?;
        self.files.coverage(record, aspect).await
    }
    pub(crate) async fn source_query(
        &self,
        project: &str,
        id: i64,
        input: AttributionInput,
        operation: String,
        query: SourceQuery,
    ) -> Result<SourceResponse> {
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let (record, run) = self
            .active_record_in(&tx, project, id, input.clone())
            .await?;
        tx.commit().await?;
        let (mut record, response) = self
            .files
            .source_query(record, run, operation, query)
            .await?;
        let tx = self.transactions.begin().await?;
        self.active_record_in(&tx, project, id, input).await?;
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await?;
        Ok(response)
    }
    pub(crate) async fn assessment(
        &self,
        project: &str,
        id: i64,
        input: AttributionInput,
        assessment: AspectAssessment,
    ) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let (mut record, run) = self
            .active_record_in(&tx, project, id, input.clone())
            .await?;
        if record.job().stage != JobStage::Discovery {
            bail!("assessments belong to discovery passes");
        }
        tx.commit().await?;
        record = self.files.assessment(record, run, assessment).await?;
        let tx = self.transactions.begin().await?;
        self.active_record_in(&tx, project, id, input).await?;
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await
    }
    pub(crate) async fn progress(
        &self,
        project: &str,
        id: i64,
        input: AttributionInput,
        progress: JobProgress,
    ) -> Result<()> {
        if progress.body.len() > 4000 {
            bail!("progress is limited to 4000 bytes");
        }
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let (mut record, _) = self.active_record_in(&tx, project, id, input).await?;
        record.job_mut().progress = progress.body;
        record.job_mut().current_area = progress.area;
        record.job_mut().current_aspect = progress.aspect;
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await
    }
    pub(crate) async fn report(
        &self,
        project: &str,
        id: i64,
        input: AttributionInput,
        report: JobReport,
    ) -> Result<()> {
        if serde_json::to_vec(&report)?.len() > 2 * 1024 * 1024 {
            bail!("job report exceeds 2 MiB");
        }
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let (mut record, run) = self
            .active_record_in(&tx, project, id, input.clone())
            .await?;
        if let Some((_, old)) = record.reports.iter().find(|(id, _)| *id == run) {
            if *old == report {
                tx.commit().await?;
                return Ok(());
            }
            bail!("this run has already submitted a different report");
        }
        tx.commit().await?;
        record = self.files.report(record, run, report).await?;
        let tx = self.transactions.begin().await?;
        self.active_record_in(&tx, project, id, input).await?;
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await
    }
    pub(crate) async fn record_read(
        &self,
        project: &str,
        input: AttributionInput,
        content: &[u8],
    ) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let attribution = self
            .attribution
            .validate_in(&tx, project, input, true)
            .await?;
        let project_id = self.projects.id_in(&tx, project).await?;
        if let Some(run) = attribution.agent_run_id
            && let Some(job_id) = self.repository.reader_job_in(&tx, project_id, run).await?
        {
            let mut record = self.repository.load_in(&tx, project_id, job_id).await?;
            if record.job().stage == JobStage::Reader && record.job().active_run_id == Some(run) {
                record.detail.quality.reading_estimated_tokens += estimated_tokens(content);
                self.persist_in(&tx, &mut record).await?;
            }
        }
        tx.commit().await
    }
    pub(crate) async fn action(
        &self,
        project: &str,
        id: i64,
        action: JobAction,
    ) -> Result<KnowledgeJob> {
        if let JobAction::Retry { request_id } | JobAction::Continue { request_id } = &action {
            let tx = self.transactions.begin().await?;
            let record = self.scoped_in(&tx, project, id).await?;
            if record.job().status.unresolved() {
                bail!("resolve the existing job or pending proposal first");
            }
            let mut request = record.job().request.clone();
            request.request_id = request_id.clone();
            request.previous_job_id = Some(id);
            tx.commit().await?;
            return self.start(project, request).await;
        }
        if action == JobAction::Apply {
            return self.apply(project, id).await;
        }
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let mut record = self.scoped_in(&tx, project, id).await?;
        let mut cancel = None;
        match action {
            JobAction::Cancel => {
                if !record.job().status.unresolved() {
                    tx.commit().await?;
                    return Ok(record.job().clone());
                }
                record.job_mut().cancel_requested = true;
                if record.job().active_run_id.is_none()
                    && record.job().stage != JobStage::Publication
                {
                    record.job_mut().status = JobStatus::Cancelled;
                    record.job_mut().outcome = "Cancelled; drafts retained".into();
                }
                cancel = record.job().active_run_id;
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
            }
            _ => unreachable!(),
        }
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await?;
        if let Some(run) = cancel {
            self.sessions.cancel_run(project, run);
        }
        Ok(record.job().clone())
    }
    pub(crate) async fn stop_project(&self, project_id: i64) -> Result<()> {
        let _lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        self.projects.name_in(&tx, project_id).await?;
        for (_, id) in self.repository.unresolved_in(&tx, Some(project_id)).await? {
            let mut record = self.repository.load_in(&tx, project_id, id).await?;
            record.job_mut().cancel_requested = true;
            record.job_mut().status = JobStatus::Cancelled;
            record.job_mut().outcome = "Project deletion stopped discovery".into();
            self.persist_in(&tx, &mut record).await?;
        }
        tx.commit().await
    }
    pub(crate) async fn settings(&self, project: &str) -> Result<KnowledgeSettings> {
        let tx = self.transactions.begin().await?;
        let id = self.projects.id_in(&tx, project).await?;
        let settings = self.repository.settings_in(&tx, id).await?;
        tx.commit().await?;
        Ok(settings)
    }
    pub(crate) async fn save_settings(
        &self,
        project: &str,
        settings: KnowledgeSettings,
    ) -> Result<KnowledgeSettings> {
        let tx = self.transactions.begin().await?;
        let id = self.projects.id_in(&tx, project).await?;
        self.repository.save_settings_in(&tx, id, &settings).await?;
        tx.commit().await?;
        Ok(settings)
    }
}
