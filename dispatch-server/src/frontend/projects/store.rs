use super::types::*;
use crate::frontend::projects::service::ProjectService;
use crate::frontend::queries::cache::QueryCache;
use dispatch_types::*;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ProjectStore {
    service: ProjectService,
    page: QueryCache<(), ProjectCatalog>,
    project_page: QueryCache<Option<String>, ProjectSettings>,
    workspace_bar: QueryCache<Option<String>, WorkspaceBarData>,
}

impl ProjectStore {
    pub(crate) fn new(service: ProjectService) -> Self {
        Self {
            service,
            page: QueryCache::persistent("dispatch.store.projects.v1"),
            project_page: QueryCache::persistent("dispatch.store.project.v1"),
            workspace_bar: QueryCache::persistent("dispatch.store.workspace-bar.v1"),
        }
    }

    pub(crate) fn cached_page(&self) -> Option<ProjectCatalog> {
        self.page.get(&())
    }

    pub(crate) fn cached_page_untracked(&self) -> Option<ProjectCatalog> {
        self.page.get_untracked(&())
    }

    pub(crate) async fn load_page(&self) -> Result<ProjectCatalog, ServerFnError> {
        let service = self.service.clone();
        self.page
            .load((), move || async move {
                service.load_page().await.map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_page(&self, value: ProjectCatalog) {
        self.page.seed((), value);
    }

    pub(crate) fn cached_project_page(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ProjectSettings> {
        self.project_page.get(&selected_project.clone())
    }

    pub(crate) fn cached_project_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ProjectSettings> {
        self.project_page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_project_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ProjectSettings, ServerFnError> {
        let service = self.service.clone();
        self.project_page
            .load(selected_project.clone(), move || async move {
                service
                    .load_project_page(selected_project)
                    .await
                    .map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_project_page(
        &self,
        selected_project: Option<String>,
        value: ProjectSettings,
    ) {
        self.project_page.seed(selected_project.clone(), value);
    }

    pub(crate) fn cached_workspace_bar(
        &self,
        selected_project: &Option<String>,
    ) -> Option<WorkspaceBarData> {
        self.workspace_bar.get(&selected_project.clone())
    }

    pub(crate) fn cached_workspace_bar_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<WorkspaceBarData> {
        self.workspace_bar.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_workspace_bar(
        &self,
        selected_project: Option<String>,
    ) -> Result<WorkspaceBarData, ServerFnError> {
        let service = self.service.clone();
        self.workspace_bar
            .load(selected_project.clone(), move || async move {
                service.load_workspace_bar(selected_project).await
            })
            .await
    }

    pub(crate) fn seed_workspace_bar(
        &self,
        selected_project: Option<String>,
        value: WorkspaceBarData,
    ) {
        self.workspace_bar.seed(selected_project.clone(), value);
    }

    pub(crate) fn projects(&self) -> Signal<Vec<ProjectView>> {
        let catalog = self.page.clone();
        Signal::derive(move || {
            catalog
                .get(&())
                .map(|catalog| catalog.projects)
                .unwrap_or_default()
        })
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn remove_deleted(&self, project_id: i64) {
        if let Some(mut catalog) = self.page.get_untracked(&()) {
            apply_project_deletion(&mut catalog.projects, project_id);
            let ticket = self.page.begin(());
            self.page.commit(ticket, catalog);
        }
    }

    pub(crate) fn invalidate_page(&self) {
        self.page.invalidate_key(&());
    }

    pub(crate) fn invalidate_project_page(&self, selected_project: Option<String>) {
        self.project_page.invalidate_key(&selected_project);
    }

    pub(crate) fn invalidate_workspace_bar(&self, selected_project: Option<String>) {
        self.workspace_bar.invalidate_key(&selected_project);
    }

    pub(crate) fn clear_cache(&self) {
        self.page.invalidate();
        self.project_page.clear();
        self.workspace_bar.clear();
    }
}

#[cfg(any(not(feature = "ssr"), test))]
fn apply_project_deletion(projects: &mut Vec<ProjectView>, deleted_project_id: i64) {
    projects.retain(|project| project.id != deleted_project_id);
}

pub(crate) fn project_store() -> ProjectStore {
    leptos::prelude::expect_context()
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;

    use super::apply_project_deletion;
    use crate::shared::view_models::{
        AgentGitCommandPolicy, AgentSandboxMode, AgentToolName, ProjectView, RevertStrategy,
        WorkspaceMode, WorktreeCleanupPolicy,
    };

    #[test]
    fn delayed_deletion_preserves_same_name_replacement() {
        let mut projects = vec![project(2, "demo"), project(3, "other")];

        apply_project_deletion(&mut projects, 1);

        assert_that!(&(project_ids(&projects))).is_equal_to(vec![2, 3]);
    }

    #[test]
    fn deletion_removes_matching_project_by_id() {
        let mut projects = vec![project(1, "demo"), project(2, "other")];

        apply_project_deletion(&mut projects, 1);

        assert_that!(&(project_ids(&projects))).is_equal_to(vec![2]);
    }

    #[test]
    fn deletion_of_an_unrelated_project_preserves_other_entries() {
        let mut projects = vec![project(1, "demo"), project(2, "other")];

        apply_project_deletion(&mut projects, 2);

        assert_that!(&(project_ids(&projects))).is_equal_to(vec![1]);
    }

    #[test]
    fn deletion_from_an_empty_cache_is_a_noop() {
        let mut projects = Vec::new();

        apply_project_deletion(&mut projects, 1);

        assert_that!(&(projects)).is_empty();
    }

    fn project_ids(projects: &[ProjectView]) -> Vec<i64> {
        projects.iter().map(|project| project.id).collect()
    }

    fn project(id: i64, name: &str) -> ProjectView {
        ProjectView {
            id,
            name: name.to_owned(),
            display_name: name.to_owned(),
            path: None,
            knowledge_directory: "knowledge".to_owned(),
            path_exists: false,
            path_checked_at: None,
            git_status: None,
            system_prompt: String::new(),
            workspace_mode: WorkspaceMode::CurrentBranch,
            max_code_edit_agents: 1,
            max_read_only_agents: 1,
            create_pr: false,
            auto_commit: false,
            commit_standard: String::new(),
            revert_strategy: RevertStrategy::Manual,
            stale_claim_minutes: 1,
            worktree_cleanup_policy: WorktreeCleanupPolicy::Manual,
            default_agent_tool: AgentToolName::Codex,
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite,
            agent_extra_writable_roots: Vec::new(),
            agent_git_command_policy: AgentGitCommandPolicy::default(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}
