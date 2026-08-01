#[cfg(feature = "ssr")]
use crate::backend::{
    app_state, automation, page_data,
    projects::{self, UpdateProjectSettings},
    workspace::{self, WorkspaceOpenTarget},
};
use crate::{
    frontend::{
        pages::{ProjectPage, ProjectsPage, WorkspaceBarData},
        services::{cache::LocalStorageCache, origin::api_base_url, request::ServiceRequest},
    },
    shared::view_models::{AgentGitCommandPolicy, HistoryClearResult, ProjectView, RevertStrategy},
};
use codee::string::JsonSerdeCodec;
use leptos::prelude::*;
use leptos_use::storage::{UseStorageOptions, use_local_storage_with_options};
use serde::{Deserialize, Serialize};

const PROJECT_CACHE_STORAGE_KEY: &str = "dispatch.projects.v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CommitPolicyUpdate {
    pub(crate) max_read_only_agents: i64,
    pub(crate) auto_commit: bool,
    pub(crate) commit_standard: String,
    pub(crate) revert_strategy: RevertStrategy,
    pub(crate) agent_git_command_policy: AgentGitCommandPolicy,
}

#[derive(Clone)]
pub(crate) struct ProjectService {
    load_page: ServiceRequest<(), ProjectsPage>,
    load_project_page: ServiceRequest<Option<String>, ProjectPage>,
    load_workspace_bar: ServiceRequest<Option<String>, WorkspaceBarData>,
    #[cfg(not(feature = "ssr"))]
    current_project_id: ServiceRequest<String, Option<i64>>,
    update_auto_commit: ServiceRequest<(String, bool), ()>,
    update_system_prompt: ServiceRequest<(String, String), ()>,
    clear_system_prompt_history: ServiceRequest<String, HistoryClearResult>,
    update_memory: ServiceRequest<(String, String), ()>,
    clear_memory_history: ServiceRequest<String, HistoryClearResult>,
    update_commit_policy: ServiceRequest<(String, CommitPolicyUpdate), ()>,
    open_workspace: ServiceRequest<(String, String), ()>,
    cleanup_worktrees: ServiceRequest<String, ()>,
    crudkit_api_base_url: String,
    cache: Option<LocalStorageCache<ProjectsPage>>,
    project_page_cache: Option<LocalStorageCache<ProjectPage>>,
    workspace_bar_cache: Option<LocalStorageCache<WorkspaceBarData>>,
}

struct ProjectRequests {
    load_page: ServiceRequest<(), ProjectsPage>,
    load_project_page: ServiceRequest<Option<String>, ProjectPage>,
    load_workspace_bar: ServiceRequest<Option<String>, WorkspaceBarData>,
    #[cfg(not(feature = "ssr"))]
    current_project_id: ServiceRequest<String, Option<i64>>,
    update_auto_commit: ServiceRequest<(String, bool), ()>,
    update_system_prompt: ServiceRequest<(String, String), ()>,
    clear_system_prompt_history: ServiceRequest<String, HistoryClearResult>,
    update_memory: ServiceRequest<(String, String), ()>,
    clear_memory_history: ServiceRequest<String, HistoryClearResult>,
    update_commit_policy: ServiceRequest<(String, CommitPolicyUpdate), ()>,
    open_workspace: ServiceRequest<(String, String), ()>,
    cleanup_worktrees: ServiceRequest<String, ()>,
}

impl ProjectService {
    fn new(requests: ProjectRequests, crudkit_api_base_url: String) -> Self {
        Self {
            load_page: requests.load_page,
            load_project_page: requests.load_project_page,
            load_workspace_bar: requests.load_workspace_bar,
            #[cfg(not(feature = "ssr"))]
            current_project_id: requests.current_project_id,
            update_auto_commit: requests.update_auto_commit,
            update_system_prompt: requests.update_system_prompt,
            clear_system_prompt_history: requests.clear_system_prompt_history,
            update_memory: requests.update_memory,
            clear_memory_history: requests.clear_memory_history,
            update_commit_policy: requests.update_commit_policy,
            open_workspace: requests.open_workspace,
            cleanup_worktrees: requests.cleanup_worktrees,
            crudkit_api_base_url,
            cache: None,
            project_page_cache: None,
            workspace_bar_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let crudkit_api_base_url = api_base_url();
        let project_page_api_base_url = crudkit_api_base_url.clone();
        let mut service = Self::new(
            ProjectRequests {
                load_page: ServiceRequest::new(|()| Box::pin(load_projects_page())),
                load_project_page: ServiceRequest::new(move |selected_project| {
                    Box::pin(load_project_page(
                        selected_project,
                        project_page_api_base_url.clone(),
                    ))
                }),
                load_workspace_bar: ServiceRequest::new(|selected_project| {
                    Box::pin(load_workspace_bar(selected_project))
                }),
                #[cfg(not(feature = "ssr"))]
                current_project_id: ServiceRequest::new(|project| {
                    Box::pin(current_project_id(project))
                }),
                update_auto_commit: ServiceRequest::new(|(project, enabled)| {
                    Box::pin(update_auto_commit(project, enabled))
                }),
                update_system_prompt: ServiceRequest::new(|(project, body)| {
                    Box::pin(update_system_prompt(project, body))
                }),
                clear_system_prompt_history: ServiceRequest::new(|project| {
                    Box::pin(clear_system_prompt_history(project))
                }),
                update_memory: ServiceRequest::new(|(project, body)| {
                    Box::pin(update_memory(project, body))
                }),
                clear_memory_history: ServiceRequest::new(|project| {
                    Box::pin(clear_memory_history(project))
                }),
                update_commit_policy: ServiceRequest::new(|(project, update)| {
                    Box::pin(update_commit_policy(project, update))
                }),
                open_workspace: ServiceRequest::new(|(project, target)| {
                    Box::pin(open_workspace(project, target))
                }),
                cleanup_worktrees: ServiceRequest::new(|project| {
                    Box::pin(cleanup_worktrees(project))
                }),
            },
            crudkit_api_base_url,
        );
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.projects.v1"));
        service.project_page_cache =
            Some(LocalStorageCache::persistent("dispatch.query.project.v1"));
        service.workspace_bar_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.workspace-bar.v1",
        ));
        service
    }

    pub(crate) fn cached_page(&self) -> Option<ProjectsPage> {
        self.cache?.get(&"page")
    }

    pub(crate) fn cached_page_untracked(&self) -> Option<ProjectsPage> {
        self.cache?.get_untracked(&"page")
    }

    pub(crate) async fn load_page(&self) -> Result<ProjectsPage, ServerFnError> {
        let lifecycle_epoch = self.cache.map(|cache| cache.capture_lifecycle_epoch());
        let page = self.load_page.execute(()).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.cache
        {
            cache.store(&"page", &page);
        }
        Ok(page)
    }

    pub(crate) fn cached_project_page(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ProjectPage> {
        self.project_page_cache?.get(&("page", selected_project))
    }

    pub(crate) fn cached_project_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ProjectPage> {
        self.project_page_cache?
            .get_untracked(&("page", selected_project))
    }

    pub(crate) async fn load_project_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ProjectPage, ServerFnError> {
        let lifecycle_epoch = self
            .project_page_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_project_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.project_page_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.project_page_cache
        {
            cache.store(&("page", key), &page);
        }
        Ok(page)
    }

    pub(crate) fn cached_workspace_bar(
        &self,
        selected_project: &Option<String>,
    ) -> Option<WorkspaceBarData> {
        self.workspace_bar_cache?
            .get(&("workspace", selected_project))
    }

    pub(crate) fn cached_workspace_bar_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<WorkspaceBarData> {
        self.workspace_bar_cache?
            .get_untracked(&("workspace", selected_project))
    }

    pub(crate) async fn load_workspace_bar(
        &self,
        selected_project: Option<String>,
    ) -> Result<WorkspaceBarData, ServerFnError> {
        let lifecycle_epoch = self
            .workspace_bar_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let data = self.load_workspace_bar.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.workspace_bar_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.workspace_bar_cache
        {
            cache.store(&("workspace", key), &data);
        }
        Ok(data)
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) async fn current_project_id(
        &self,
        project: String,
    ) -> Result<Option<i64>, ServerFnError> {
        self.current_project_id.execute(project).await
    }

    pub(crate) async fn update_auto_commit(
        &self,
        project: String,
        enabled: bool,
    ) -> Result<(), ServerFnError> {
        self.update_auto_commit.execute((project, enabled)).await
    }

    pub(crate) async fn update_system_prompt(
        &self,
        project: String,
        body: String,
    ) -> Result<(), ServerFnError> {
        self.update_system_prompt.execute((project, body)).await
    }

    pub(crate) async fn clear_system_prompt_history(
        &self,
        project: String,
    ) -> Result<HistoryClearResult, ServerFnError> {
        self.clear_system_prompt_history.execute(project).await
    }

    pub(crate) async fn update_memory(
        &self,
        project: String,
        body: String,
    ) -> Result<(), ServerFnError> {
        self.update_memory.execute((project, body)).await
    }

    pub(crate) async fn clear_memory_history(
        &self,
        project: String,
    ) -> Result<HistoryClearResult, ServerFnError> {
        self.clear_memory_history.execute(project).await
    }

    pub(crate) async fn update_commit_policy(
        &self,
        project: String,
        update: CommitPolicyUpdate,
    ) -> Result<(), ServerFnError> {
        self.update_commit_policy.execute((project, update)).await
    }

    pub(crate) async fn open_workspace(
        &self,
        project: String,
        target: String,
    ) -> Result<(), ServerFnError> {
        self.open_workspace.execute((project, target)).await
    }

    pub(crate) async fn cleanup_worktrees(&self, project: String) -> Result<(), ServerFnError> {
        self.cleanup_worktrees.execute(project).await
    }

    pub(crate) fn crudkit_api_base_url(&self) -> &str {
        &self.crudkit_api_base_url
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.cache {
            cache.clear();
        }
        if let Some(cache) = self.project_page_cache {
            cache.clear();
        }
        if let Some(cache) = self.workspace_bar_cache {
            cache.clear();
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ProjectCache {
    projects: Signal<Vec<ProjectView>>,
    set_projects: WriteSignal<Vec<ProjectView>>,
}

#[server(prefix = "/leptos")]
async fn load_projects_page() -> Result<ProjectsPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::projects_page_data(&state.store, &state.automation_controller, codex_status)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_project_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<ProjectPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::project_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
        api_base_url,
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_workspace_bar(
    selected_project: Option<String>,
) -> Result<WorkspaceBarData, ServerFnError> {
    let state = app_state::app_state();
    page_data::workspace_bar_data(&state.store, selected_project.as_deref())
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn current_project_id(project: String) -> Result<Option<i64>, ServerFnError> {
    let state = app_state::app_state();
    projects::find_project_id_by_name(&state.store, &project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_auto_commit(project: String, enabled: bool) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    projects::update_settings(
        &state.store,
        &project,
        UpdateProjectSettings {
            auto_commit: Some(enabled),
            ..Default::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_system_prompt(project: String, body: String) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    projects::update_system_prompt(&state.store, &project, body)
        .await
        .map(|_| ())
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn clear_system_prompt_history(project: String) -> Result<HistoryClearResult, ServerFnError> {
    let state = app_state::app_state();
    projects::clear_system_prompt_history(&state.store, &project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_memory(project: String, body: String) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    projects::update_memory_with_source(
        &state.store,
        &project,
        body,
        projects::ProjectChangeSource::User,
    )
    .await
    .map(|_| ())
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn clear_memory_history(project: String) -> Result<HistoryClearResult, ServerFnError> {
    let state = app_state::app_state();
    projects::clear_memory_history(&state.store, &project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_commit_policy(
    project: String,
    update: CommitPolicyUpdate,
) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    projects::update_settings(
        &state.store,
        &project,
        UpdateProjectSettings {
            max_read_only_agents: Some(update.max_read_only_agents),
            auto_commit: Some(update.auto_commit),
            commit_standard: Some(update.commit_standard),
            revert_strategy: Some(update.revert_strategy),
            agent_git_command_policy: Some(update.agent_git_command_policy),
            ..Default::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn open_workspace(project: String, target: String) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    let target =
        WorkspaceOpenTarget::parse(&target).map_err(|err| ServerFnError::new(err.to_string()))?;
    let path = workspace::project_workspace_path(&state.store, &project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))?;
    workspace::open_workspace_path(target, path)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn cleanup_worktrees(project: String) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    automation::cleanup_worktrees(&state.store, &project, None)
        .await
        .map(|_| ())
        .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn provide_project_cache() {
    let (projects, set_projects, _) =
        use_local_storage_with_options::<Vec<ProjectView>, JsonSerdeCodec>(
            PROJECT_CACHE_STORAGE_KEY,
            UseStorageOptions::default().delay_during_hydration(true),
        );
    provide_context(ProjectCache {
        projects,
        set_projects,
    });
}

pub(crate) fn project_cache() -> ProjectCache {
    expect_context::<ProjectCache>()
}

impl ProjectCache {
    pub(crate) fn track<T>(
        self,
        value: ReadSignal<Option<T>>,
        projects: impl for<'a> Fn(&'a T) -> &'a [ProjectView] + Copy + 'static,
    ) where
        T: Clone + Send + Sync + 'static,
    {
        Effect::new(move |_| {
            if let Some(value) = value.get() {
                self.store(projects(&value));
            }
        });
    }

    pub(crate) fn projects(self) -> Signal<Vec<ProjectView>> {
        self.projects
    }

    fn store(self, projects: &[ProjectView]) {
        if self.projects.with_untracked(|cached| cached != projects) {
            self.set_projects.set(projects.to_vec());
        }
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn remove_deleted(self, project_id: i64) {
        self.set_projects
            .update(|projects| apply_project_deletion(projects, project_id));
    }
}

#[cfg(any(not(feature = "ssr"), test))]
fn apply_project_deletion(projects: &mut Vec<ProjectView>, deleted_project_id: i64) {
    projects.retain(|project| project.id != deleted_project_id);
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
            path_exists: false,
            path_checked_at: None,
            git_status: None,
            system_prompt: String::new(),
            memory: String::new(),
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
