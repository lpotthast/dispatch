use super::{
    model::{CreateProject, ProjectChangeSource, UpdateProject},
    repository::{ProjectDefaultsRepository, ProjectRepository},
    runtime::ProjectRuntime,
    settings::{self, UpdateProjectSettings},
};
use crate::backend::{
    events::UiEventBus,
    storage::{TransactionManager, utc_now},
};
use dispatch_types::{
    AgentGitCommandPolicy, AgentSandboxMode, AgentToolName, CodexAgentModel, HistoryClearResult,
    ProjectSettingsView, ProjectSystemPromptEventView, ProjectSystemPromptUpdateView, ProjectView,
    RevertStrategy, WorkspaceMode, WorktreeCleanupPolicy,
};
use rootcause::{Result, prelude::*};
use std::{path::PathBuf, sync::Arc};

/// Authoritative project configuration, settings, prompt history, and path health operations.
pub(crate) struct ProjectService {
    transactions: Arc<TransactionManager>,
    repository: Arc<ProjectRepository>,
    defaults: Arc<ProjectDefaultsRepository>,
    runtime: Arc<ProjectRuntime>,
    events: UiEventBus,
    database_path: PathBuf,
}

impl ProjectService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        repository: Arc<ProjectRepository>,
        defaults: Arc<ProjectDefaultsRepository>,
        runtime: Arc<ProjectRuntime>,
        events: UiEventBus,
        database_path: PathBuf,
    ) -> Self {
        Self {
            transactions,
            repository,
            defaults,
            runtime,
            events,
            database_path,
        }
    }

    fn with_git_status(&self, mut project: ProjectView) -> ProjectView {
        project.git_status = self
            .runtime
            .git_status(project.path.as_deref(), project.path_exists);
        project
    }

    pub(crate) async fn list(&self) -> Result<Vec<ProjectView>> {
        crate::backend::metrics::time_repository("projects.list_with_git_status", async {
            Ok(self
                .repository
                .list()
                .await?
                .into_iter()
                .map(|project| self.with_git_status(project))
                .collect())
        })
        .await
    }

    /// Page shells omit Git inspection; the workspace bar loads it independently.
    pub(crate) async fn list_summaries(&self) -> Result<Vec<ProjectView>> {
        crate::backend::metrics::time_repository("projects.list_summaries", self.repository.list())
            .await
    }

    pub(crate) async fn get(&self, name: &str) -> Result<ProjectView> {
        Ok(self.with_git_status(self.repository.by_name(name).await?))
    }

    pub(crate) async fn find_id(&self, name: &str) -> Result<Option<i64>> {
        self.repository.find_id(name).await
    }
    pub(crate) async fn id(&self, name: &str) -> Result<i64> {
        self.repository.id(name).await
    }
    pub(crate) async fn settings(&self, name: &str) -> Result<ProjectSettingsView> {
        self.repository.settings(name).await
    }

    fn prepare_create(&self, create: CreateProject) -> Result<(ProjectView, String)> {
        settings::validate_project_name(&create.name)?;
        let display_name = create
            .display_name
            .unwrap_or_else(|| create.name.clone())
            .trim()
            .to_owned();
        if display_name.is_empty() {
            bail!("project display name cannot be empty");
        }
        let path = self.runtime.normalize_path(create.path)?;
        let default_agent_model = settings::normalize_optional(create.default_agent_model)
            .or_else(|| Some(CodexAgentModel::newest().as_storage().to_owned()));
        let default_agent_reasoning_effort =
            Some(create.default_agent_reasoning_effort.unwrap_or_else(|| {
                settings::default_reasoning_effort_for_model(default_agent_model.as_deref())
            }));
        let now = utc_now();
        let project = ProjectView {
            id: 0,
            name: create.name,
            display_name,
            path: Some(path),
            path_exists: true,
            path_checked_at: Some(now.clone()),
            git_status: None,
            knowledge_directory: crate::backend::knowledge::DEFAULT_KNOWLEDGE_DIRECTORY.to_owned(),
            system_prompt: create.system_prompt.unwrap_or_default(),
            workspace_mode: WorkspaceMode::CurrentBranch,
            max_code_edit_agents: 1,
            max_read_only_agents: 2,
            create_pr: false,
            auto_commit: true,
            commit_standard: String::new(),
            revert_strategy: RevertStrategy::Manual,
            stale_claim_minutes: 0,
            worktree_cleanup_policy: WorktreeCleanupPolicy::Manual,
            default_agent_tool: AgentToolName::Codex,
            default_agent_model,
            default_agent_reasoning_effort,
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite,
            agent_extra_writable_roots: vec![],
            agent_git_command_policy: AgentGitCommandPolicy::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        settings::validate_project_settings(&project)?;
        Ok((project, create.memory.unwrap_or_default()))
    }

    pub(crate) fn validate_create(&self, create: CreateProject) -> Result<()> {
        self.prepare_create(create).map(|_| ())
    }

    pub(crate) fn validate_edit(
        &self,
        existing: &ProjectView,
        update: UpdateProject,
        settings: UpdateProjectSettings,
    ) -> Result<()> {
        if update
            .display_name
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("project display name cannot be empty");
        }
        if let Some(path) = update.path {
            self.runtime.prepare_path_update(path)?;
        }
        if let Some(directory) = &settings.knowledge_directory {
            self.runtime
                .validate_knowledge_directory(existing, directory)?;
        }
        settings::apply_update(settings, existing, &self.database_path)?;
        Ok(())
    }

    pub(crate) async fn create(&self, create: CreateProject) -> Result<ProjectView> {
        let (project, memory) = self.prepare_create(create)?;
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start project create")?;
        let project = self
            .repository
            .insert_in(&transaction, project, memory)
            .await?;
        self.defaults.initialize_in(&transaction, &project).await?;
        if !project.system_prompt.trim().is_empty() {
            self.repository
                .record_prompt_in(
                    &transaction,
                    &project,
                    "initial",
                    &ProjectChangeSource::System,
                )
                .await?;
        }
        transaction
            .commit()
            .await
            .context("failed to commit project create")?;
        self.events.publish_project_list_changed();
        self.events.publish_project_changed(&project.name);
        Ok(self.with_git_status(project))
    }

    pub(crate) async fn update(&self, name: &str, update: UpdateProject) -> Result<ProjectView> {
        if update.display_name.is_none() && update.path.is_none() {
            bail!("project update requires at least one field");
        }
        Ok(self.with_git_status(self.edit(name, None, update, None).await?))
    }

    /// Admin editing coordinates configuration and settings in the same transaction.
    /// The immutable identity fences a deleted project from a replacement with the same name.
    pub(crate) async fn edit(
        &self,
        name: &str,
        expected_id: Option<i64>,
        update: UpdateProject,
        settings_update: Option<UpdateProjectSettings>,
    ) -> Result<ProjectView> {
        let configuration_updated = update.display_name.is_some() || update.path.is_some();
        let path = update
            .path
            .map(|path| self.runtime.prepare_path_update(path))
            .transpose()?;
        let display_name = update.display_name.map(|name| name.trim().to_owned());
        if display_name.as_ref().is_some_and(String::is_empty) {
            bail!("project display name cannot be empty");
        }
        // Inspect the directory before taking the database connection, then fence that scope below.
        let checked_scope = if let Some(directory) = settings_update
            .as_ref()
            .and_then(|update| update.knowledge_directory.as_ref())
        {
            let existing = self.repository.by_name_for_edit(name).await?;
            self.runtime
                .validate_knowledge_directory(&existing, directory)?;
            Some((existing.id, existing.path, existing.knowledge_directory))
        } else {
            None
        };
        let transaction = self.transactions.begin().await?;
        let mut project = self
            .repository
            .by_name_for_edit_in(&transaction, name)
            .await?;
        if expected_id.is_some_and(|id| id != project.id) {
            bail!("project '{name}' changed while it was being edited");
        }
        if let Some((id, checked_path, directory)) = checked_scope
            && (project.id != id
                || project.path != checked_path
                || project.knowledge_directory != directory)
        {
            bail!(
                "project '{name}' changed while its knowledge directory was being checked; reload it"
            );
        }
        if let Some(update) = settings_update {
            project = settings::apply_update(update, &project, &self.database_path)?;
        }
        if let Some(display_name) = display_name {
            project.display_name = display_name;
        }
        if let Some(path) = path
            && project.path != path
        {
            project.path_exists = path.is_some();
            project.path = path;
            project.path_checked_at = Some(utc_now());
        }
        settings::validate_project_settings(&project)?;
        project.updated_at = utc_now();
        let project = self.repository.save_in(&transaction, project).await?;
        transaction.commit().await?;
        if configuration_updated {
            self.events.publish_project_list_changed();
        }
        self.events.publish_project_changed(name);
        Ok(project)
    }

    pub(crate) async fn update_settings(
        &self,
        name: &str,
        update: UpdateProjectSettings,
    ) -> Result<ProjectSettingsView> {
        Ok(settings::project_settings_to_view(
            self.edit(name, None, UpdateProject::default(), Some(update))
                .await?,
        ))
    }

    pub(crate) async fn update_system_prompt(
        &self,
        name: &str,
        body: String,
    ) -> Result<ProjectView> {
        Ok(self
            .update_system_prompt_with_source(name, body, ProjectChangeSource::User)
            .await?
            .project)
    }

    pub(crate) async fn update_system_prompt_with_source(
        &self,
        name: &str,
        body: String,
        source: ProjectChangeSource,
    ) -> Result<ProjectSystemPromptUpdateView> {
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start project system prompt update")?;
        let mut project = self.repository.by_name_in(&transaction, name).await?;
        project.system_prompt = body;
        project.updated_at = utc_now();
        let project = self.repository.save_in(&transaction, project).await?;
        let event = self
            .repository
            .record_prompt_in(&transaction, &project, "set", &source)
            .await?;
        transaction
            .commit()
            .await
            .context("failed to commit project system prompt update")?;
        self.events.publish_system_prompt_changed(name);
        Ok(ProjectSystemPromptUpdateView {
            project: self.with_git_status(project),
            event,
        })
    }

    pub(crate) async fn system_prompt_events(
        &self,
        name: &str,
    ) -> Result<Vec<ProjectSystemPromptEventView>> {
        let transaction = self.transactions.begin().await?;
        let project = self.repository.by_name_in(&transaction, name).await?;
        let history = self
            .repository
            .prompt_history_in(&transaction, &project)
            .await?;
        transaction.commit().await?;
        Ok(history)
    }

    pub(crate) async fn clear_system_prompt_history(
        &self,
        name: &str,
    ) -> Result<HistoryClearResult> {
        let transaction = self.transactions.begin().await?;
        let id = self.repository.id_in(&transaction, name).await?;
        let deleted_events = self
            .repository
            .clear_prompt_history_in(&transaction, id)
            .await?;
        transaction.commit().await?;
        self.events.publish_system_prompt_changed(name);
        Ok(HistoryClearResult { deleted_events })
    }

    pub(crate) async fn refresh_path_statuses(&self) -> Result<Vec<ProjectView>> {
        let mut refreshed = vec![];
        for observed in self.repository.list().await? {
            let exists = self.runtime.path_exists(observed.path.as_deref());
            let checked_at = utc_now();
            let transaction = self.transactions.begin().await?;
            let mut current = self.repository.by_id_in(&transaction, observed.id).await?;
            // Do not attach a filesystem result to a path that changed while it was inspected.
            if current.path == observed.path {
                current.path_exists = exists;
                current.path_checked_at = Some(checked_at);
                current = self.repository.save_in(&transaction, current).await?;
            }
            transaction.commit().await?;
            refreshed.push(self.with_git_status(current));
        }
        Ok(refreshed)
    }
}
