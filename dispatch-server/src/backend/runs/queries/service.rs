use super::{
    model::{ItemRunPreviews, RunFilter},
    repository::RunQueryRepository,
    runtime::RunArtifacts,
};
use crate::backend::{
    execution::sessions::ProcessSessionRegistry, execution::tools::service::ToolService,
    projects::repository::ProjectRepository, runs::admission::repository::RunAdmissionRepository,
    storage::TransactionManager,
};
use dispatch_types::{AgentRunView, AutomationStatusView, RunLogView};
use rootcause::Result;
use std::{collections::HashMap, sync::Arc};
pub(crate) struct RunQueryService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RunQueryRepository>,
    artifacts: Arc<RunArtifacts>,
    sessions: ProcessSessionRegistry,
    tools: Arc<ToolService>,
    admission: Arc<RunAdmissionRepository>,
}
impl RunQueryService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RunQueryRepository>,
        artifacts: Arc<RunArtifacts>,
        sessions: ProcessSessionRegistry,
        tools: Arc<ToolService>,
        admission: Arc<RunAdmissionRepository>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            artifacts,
            sessions,
            tools,
            admission,
        }
    }
    async fn enrich(&self, records: Vec<AgentRunView>) -> Vec<AgentRunView> {
        let mut views = Vec::with_capacity(records.len());
        for record in records {
            views.push(self.artifacts.enrich(record).await);
        }
        views
    }
    pub(crate) async fn active_sessions(
        &self,
        project: &str,
    ) -> Result<Vec<dispatch_types::ProcessSessionView>> {
        let tx = self.transactions.begin().await?;
        let id = self.projects.id_in(&tx, project).await?;
        tx.commit().await?;
        Ok(self.sessions.list_for_project(id))
    }
    pub(crate) async fn list(
        &self,
        project: &str,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        self.list_filtered(project, RunFilter::Project, limit).await
    }
    pub(crate) async fn list_for_item(
        &self,
        project: &str,
        item_id: i64,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        self.list_filtered(project, RunFilter::Item(item_id), limit)
            .await
    }
    pub(crate) async fn list_for_trigger(
        &self,
        project: &str,
        trigger_id: i64,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        self.list_filtered(project, RunFilter::Trigger(trigger_id), limit)
            .await
    }
    async fn list_filtered(
        &self,
        project: &str,
        filter: RunFilter,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        let tx = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&tx, project).await?;
        let records = self
            .repository
            .list_in(&tx, project_id, filter, limit)
            .await?;
        tx.commit().await?;
        Ok(self.enrich(records).await)
    }
    pub(crate) async fn list_for_project_id(
        &self,
        project_id: i64,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        let tx = self.transactions.begin().await?;
        let records = self
            .repository
            .list_in(&tx, project_id, RunFilter::Project, limit)
            .await?;
        tx.commit().await?;
        Ok(self.enrich(records).await)
    }
    pub(crate) async fn get(&self, project: &str, run_id: i64) -> Result<AgentRunView> {
        let tx = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&tx, project).await?;
        let record = self.repository.get_in(&tx, project_id, run_id).await?;
        tx.commit().await?;
        Ok(self.artifacts.enrich(record).await)
    }
    pub(crate) async fn previews_for_project_id(
        &self,
        project_id: i64,
        item_ids: &[i64],
    ) -> Result<HashMap<i64, ItemRunPreviews>> {
        let tx = self.transactions.begin().await?;
        let records = self
            .repository
            .previews_in(&tx, project_id, item_ids)
            .await?;
        tx.commit().await?;
        Ok(records)
    }
    #[cfg(test)]
    pub(crate) async fn previews(
        &self,
        project: &str,
        item_ids: &[i64],
    ) -> Result<HashMap<i64, ItemRunPreviews>> {
        let tx = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&tx, project).await?;
        let records = self
            .repository
            .previews_in(&tx, project_id, item_ids)
            .await?;
        tx.commit().await?;
        Ok(records)
    }
    pub(crate) async fn active_project_names(&self) -> Result<Vec<String>> {
        let tx = self.transactions.begin().await?;
        let names = self.repository.active_project_names_in(&tx).await?;
        tx.commit().await?;
        Ok(names)
    }
    pub(crate) async fn log(&self, project: &str, run_id: i64) -> Result<RunLogView> {
        let tx = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&tx, project).await?;
        let run = self.repository.get_in(&tx, project_id, run_id).await?;
        let created_items = self
            .repository
            .item_summaries_in(project_id, &tx, run_id, true)
            .await?;
        let modified_items = self
            .repository
            .item_summaries_in(project_id, &tx, run_id, false)
            .await?;
        tx.commit().await?;
        let run = self.artifacts.enrich(run).await;
        let mut log = RunLogView {
            run,
            active: false,
            developer_instructions: None,
            user_prompt: None,
            output: Vec::new(),
            created_items,
            modified_items,
        };
        self.artifacts.read_log(&mut log).await?;
        if let Some(session) = self.sessions.get_for_project(project_id, run_id) {
            log.active = true;
            if !session.output.is_empty() {
                log.output = session.output;
            }
        }
        Ok(log)
    }
    pub(crate) async fn status_for_project_id(
        &self,
        project: &str,
        project_id: i64,
    ) -> Result<AutomationStatusView> {
        // Tool discovery is independent of project persistence; no pooled call is nested inside this transaction.
        let tools = self.tools.list().await?;
        let tx = self.transactions.begin().await?;
        let settings = self.projects.settings_by_id_in(&tx, project_id).await?;
        let counts = self.admission.counts_in(&tx, project_id).await?;
        let recent_runs = self
            .repository
            .list_in(&tx, project_id, RunFilter::Project, Some(10))
            .await?;
        tx.commit().await?;
        Ok(AutomationStatusView {
            project: project.into(),
            allowed_mutating_runs: crate::backend::projects::allowed_code_edit_agents(&settings),
            settings,
            running_runs: counts.total(),
            running_mutating_runs: counts.mutating,
            running_read_only_runs: counts.read_only,
            recent_runs: self.enrich(recent_runs).await,
            tools,
        })
    }
    #[cfg(test)]
    pub(crate) async fn status(&self, project: &str) -> Result<AutomationStatusView> {
        let id = self.projects.id(project).await?;
        self.status_for_project_id(project, id).await
    }
}
