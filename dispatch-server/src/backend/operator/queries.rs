use crate::backend::{
    automation::supervisor::AutomationSupervisor, comments,
    execution::sessions::ProcessSessionRegistry, relationships,
};
use rootcause::Result;
use std::{collections::HashSet, sync::Arc};
use {crate::shared::view_models::AgentRunView, dispatch_types::*};
pub(crate) struct OperatorQueryService {
    codex: Arc<crate::backend::execution::codex::service::CodexService>,
    run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
    state_service: Arc<crate::backend::items::states::service::StateService>,
    label_service: Arc<crate::backend::items::labels::service::LabelService>,
    item_service: Arc<crate::backend::items::service::ItemService>,
    project_service: Arc<crate::backend::projects::service::ProjectService>,
    automation_supervisor: AutomationSupervisor,
    codex_status: crate::backend::execution::codex::model::SharedCodexStatus,
    admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
    sessions: ProcessSessionRegistry,
    comment_service: Arc<comments::service::CommentService>,
    relationship_service: Arc<relationships::service::RelationshipService>,
    workspaces: Arc<crate::backend::execution::workspaces::service::WorkspaceService>,
    personality_service:
        Arc<crate::backend::automation::personalities::service::PersonalityService>,
}
impl OperatorQueryService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        codex: Arc<crate::backend::execution::codex::service::CodexService>,
        run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
        state_service: Arc<crate::backend::items::states::service::StateService>,
        label_service: Arc<crate::backend::items::labels::service::LabelService>,
        item_service: Arc<crate::backend::items::service::ItemService>,
        project_service: Arc<crate::backend::projects::service::ProjectService>,
        automation_supervisor: AutomationSupervisor,
        codex_status: crate::backend::execution::codex::model::SharedCodexStatus,
        admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
        sessions: ProcessSessionRegistry,
        comment_service: Arc<comments::service::CommentService>,
        relationship_service: Arc<relationships::service::RelationshipService>,
        workspaces: Arc<crate::backend::execution::workspaces::service::WorkspaceService>,
        personality_service: Arc<
            crate::backend::automation::personalities::service::PersonalityService,
        >,
    ) -> Self {
        Self {
            codex,
            run_queries,
            state_service,
            label_service,
            item_service,
            project_service,
            automation_supervisor,
            codex_status,
            admission,
            sessions,
            comment_service,
            relationship_service,
            workspaces,
            personality_service,
        }
    }
    pub(crate) async fn runs_section(&self, project: &str) -> Result<RunsSection> {
        let project_id = self.project_service.id(project).await?;
        let (running, recent_runs) = tokio::try_join!(
            self.admission.running_counts_for_project_id(project_id),
            self.run_queries.list_for_project_id(project_id, Some(10)),
        )?;
        let automation_running = self
            .automation_supervisor
            .is_project_running(project_id)
            .await;
        let active_run_ids = self.sessions.active_run_ids_for_project(project_id);
        let runs = self
            .run_summaries(project, recent_runs, active_run_ids)
            .await?;

        Ok(RunsSection {
            automation_running,
            running_runs: running.total(),
            running_mutating_runs: running.mutating,
            running_read_only_runs: running.read_only,
            runs,
        })
    }
    pub(crate) async fn run_summaries(
        &self,
        project: &str,
        recent_runs: Vec<AgentRunView>,
        active_run_ids: Vec<i64>,
    ) -> Result<Vec<RunSummaryView>> {
        let active_ids = active_run_ids.iter().copied().collect::<HashSet<_>>();
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();

        for run in recent_runs
            .iter()
            .filter(|run| active_ids.contains(&run.id))
            .cloned()
        {
            seen.insert(run.id);
            ordered.push(run);
        }
        let missing_active_ids = active_run_ids
            .into_iter()
            .filter(|run_id| !seen.contains(run_id))
            .collect::<Vec<_>>();
        for run_id in missing_active_ids {
            let run = self.run_queries.get(project, run_id).await?;
            seen.insert(run.id);
            ordered.push(run);
        }
        for run in recent_runs
            .into_iter()
            .filter(|run| !seen.contains(&run.id))
        {
            ordered.push(run);
        }

        Ok(ordered
            .into_iter()
            .map(|run| RunSummaryView {
                active: active_ids.contains(&run.id),
                run,
            })
            .collect())
    }
    pub(crate) async fn rule_runs(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Result<Vec<RunSummaryView>> {
        let project_id = self.project_service.id(project).await?;
        let runs = self
            .run_queries
            .list_for_trigger(project, trigger_id, None)
            .await?;
        let run_ids = runs.iter().map(|run| run.id).collect::<HashSet<_>>();
        let active_run_ids = self
            .sessions
            .active_run_ids_for_project(project_id)
            .into_iter()
            .filter(|run_id| run_ids.contains(run_id))
            .collect::<Vec<_>>();
        self.run_summaries(project, runs, active_run_ids).await
    }
    pub(crate) async fn item_page(
        &self,
        project: &str,
        item_id: i64,
        api_base_url: String,
    ) -> Result<ItemPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let item = self.item_service.get(project, item_id).await?;
        let comments = self.comment_service.list(project, item_id).await?;
        let relationships = self.relationship_service.list(project, item_id).await?;
        let label_suggestions = self.label_service.project_labels(project).await?;
        let work_item_states = self.state_service.list(project).await?;
        let automation_runs = self
            .run_queries
            .list_for_item(project, item_id, Some(10))
            .await?;
        Ok(ItemPage {
            projects,
            active_project_names,
            project: project.to_owned(),
            item,
            comments,
            relationships,
            label_suggestions,
            work_item_states,
            automation_runs,
            api_base_url,
            codex_status,
        })
    }
    pub(crate) async fn run_log_page(&self, project: &str, run_id: i64) -> Result<RunLogPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let run_log = self.run_queries.log(project, run_id).await?;
        Ok(RunLogPage {
            projects,
            active_project_names,
            project: project.to_owned(),
            run_log,
            codex_status,
        })
    }
    pub(crate) async fn projects_page(&self) -> Result<ProjectsPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;

        Ok(ProjectsPage {
            projects,
            active_project_names,
            codex_status,
        })
    }
    pub(crate) async fn workspace_bar(
        &self,
        selected_project: Option<&str>,
    ) -> Result<WorkspaceBarData> {
        let project = match selected_project {
            Some(project) => Some(self.project_service.get(project).await?),
            None => None,
        };

        Ok(WorkspaceBarData {
            project,
            workspace_editors: self.workspaces.available_editors(),
        })
    }
    pub(crate) async fn project_page(
        &self,
        selected_project: Option<&str>,
        api_base_url: String,
    ) -> Result<ProjectPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let selected_project = selected_project
            .filter(|selected| projects.iter().any(|project| project.name == *selected))
            .map(ToOwned::to_owned);
        let selected_project_view = selected_project
            .as_deref()
            .and_then(|project| projects.iter().find(|candidate| candidate.name == project))
            .cloned();
        let system_prompt_events = if let Some(project) = selected_project.as_deref() {
            self.project_service.system_prompt_events(project).await?
        } else {
            Vec::new()
        };

        Ok(ProjectPage {
            projects,
            active_project_names,
            selected_project,
            selected_project_view,
            system_prompt_events,
            api_base_url,
            codex_status,
        })
    }
    pub(crate) async fn automation_page(
        &self,
        selected_project: Option<&str>,
        api_base_url: String,
    ) -> Result<TriggersPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let selected_project = selected_project
            .filter(|selected| projects.iter().any(|project| project.name == *selected))
            .map(ToOwned::to_owned);
        let selected_project_view = selected_project
            .as_deref()
            .and_then(|project| projects.iter().find(|candidate| candidate.name == project))
            .cloned();
        let project_personalities = if let Some(project) = selected_project_view
            .as_ref()
            .map(|project| project.name.as_str())
        {
            self.personality_service.list(project).await?
        } else {
            Vec::new()
        };
        let settings = if let Some(project) = selected_project.as_deref() {
            Some(self.project_service.settings(project).await?)
        } else {
            None
        };

        Ok(TriggersPage {
            projects,
            active_project_names,
            selected_project,
            selected_project_view,
            settings,
            personalities: project_personalities,
            api_base_url,
            codex_status,
        })
    }
    pub(crate) async fn codex_page(
        &self,
        selected_project: Option<&str>,
    ) -> Result<CodexStatusPage> {
        let codex_status = self
            .codex
            .refresh_if_stale(&self.codex_status, std::time::Duration::from_secs(4 * 60))
            .await;

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let selected_project = selected_project
            .filter(|selected| projects.iter().any(|project| project.name == *selected))
            .map(ToOwned::to_owned);

        Ok(CodexStatusPage {
            projects,
            active_project_names,
            selected_project,
            codex_status,
        })
    }
    pub(crate) async fn metrics_page(
        &self,
        selected_project: Option<&str>,
    ) -> Result<MetricsPageData> {
        let codex_status = self.codex_status.read().await.clone();

        let (projects, active_project_names) = tokio::try_join!(
            self.project_service.list_summaries(),
            self.active_projects(),
        )?;
        let selected_project = selected_project
            .filter(|selected| projects.iter().any(|project| project.name == *selected))
            .map(ToOwned::to_owned);

        Ok(MetricsPageData {
            projects,
            active_project_names,
            selected_project,
            codex_status,
            metrics: crate::backend::metrics::snapshot(),
        })
    }
    pub(crate) async fn api_docs_page(
        &self,
        selected_project: Option<&str>,
    ) -> Result<ApiDocsPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.active_projects().await?;
        let selected_project = selected_project
            .filter(|selected| projects.iter().any(|project| project.name == *selected))
            .map(ToOwned::to_owned);

        Ok(ApiDocsPage {
            projects,
            active_project_names,
            selected_project,
            codex_status,
        })
    }
    pub(crate) async fn active_projects(&self) -> Result<Vec<String>> {
        let mut active = self.run_queries.active_project_names().await?;
        active.extend(self.automation_supervisor.active_project_names().await);
        active.sort();
        active.dedup();
        Ok(active)
    }
}
