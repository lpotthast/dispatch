use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use git2::{DiffOptions, ErrorCode as GitErrorCode, Oid, Repository};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};

use crate::{
    backend::{
        automation_triggers,
        entities::project::{self, Project, ProjectActiveModel, ProjectModel},
        events, personalities,
        storage::{Store, utc_now},
        swim_lanes, work_item_states,
    },
    shared::view_models::{
        AgentGitCommandPolicy, AgentReasoningEffort, AgentSandboxMode, AgentToolName,
        CodexAgentModel, ProjectGitStatusView, ProjectMemoryCompactionView, ProjectMemoryEventView,
        ProjectMemoryUpdateView, ProjectMemoryView, ProjectSettingsView,
        ProjectSystemPromptCompactionView, ProjectSystemPromptEventView,
        ProjectSystemPromptUpdateView, ProjectView, RevertStrategy, WorkspaceMode,
        WorktreeCleanupPolicy,
    },
};

mod change_events;

pub use change_events::ProjectChangeSource;

const PROJECT_PATH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct CreateProject {
    pub name: String,
    pub display_name: Option<String>,
    pub path: PathBuf,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub system_prompt: Option<String>,
    pub memory: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateProject {
    pub display_name: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateProjectSettings {
    pub workspace_mode: Option<WorkspaceMode>,
    pub max_code_edit_agents: Option<i64>,
    pub max_read_only_agents: Option<i64>,
    pub create_pr: Option<bool>,
    pub auto_commit: Option<bool>,
    pub commit_standard: Option<String>,
    pub revert_strategy: Option<RevertStrategy>,
    pub stale_claim_minutes: Option<i64>,
    pub worktree_cleanup_policy: Option<WorktreeCleanupPolicy>,
    pub default_agent_tool: Option<AgentToolName>,
    pub default_agent_model: Option<Option<String>>,
    pub default_agent_reasoning_effort: Option<Option<AgentReasoningEffort>>,
    pub agent_sandbox_mode: Option<AgentSandboxMode>,
    pub agent_extra_writable_roots: Option<Vec<String>>,
    pub agent_git_command_policy: Option<AgentGitCommandPolicy>,
}

/// Fully decoded project settings used after crossing the text-based SeaORM boundary.
///
/// Keeping this representation separate from `ProjectModel` makes invalid persisted enum and JSON
/// values ordinary service errors. They never reach workflow code and never panic view creation.
#[derive(Clone, Debug)]
struct ValidatedProjectSettings {
    workspace_mode: WorkspaceMode,
    max_code_edit_agents: i64,
    max_read_only_agents: i64,
    create_pr: bool,
    auto_commit: bool,
    commit_standard: String,
    revert_strategy: RevertStrategy,
    stale_claim_minutes: i64,
    worktree_cleanup_policy: WorktreeCleanupPolicy,
    default_agent_tool: AgentToolName,
    default_agent_model: Option<String>,
    default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    agent_sandbox_mode: AgentSandboxMode,
    agent_extra_writable_roots: Vec<String>,
    agent_git_command_policy: AgentGitCommandPolicy,
}

impl UpdateProjectSettings {
    fn has_any_field(&self) -> bool {
        self.workspace_mode.is_some()
            || self.max_code_edit_agents.is_some()
            || self.max_read_only_agents.is_some()
            || self.create_pr.is_some()
            || self.auto_commit.is_some()
            || self.commit_standard.is_some()
            || self.revert_strategy.is_some()
            || self.stale_claim_minutes.is_some()
            || self.worktree_cleanup_policy.is_some()
            || self.default_agent_tool.is_some()
            || self.default_agent_model.is_some()
            || self.default_agent_reasoning_effort.is_some()
            || self.agent_sandbox_mode.is_some()
            || self.agent_extra_writable_roots.is_some()
            || self.agent_git_command_policy.is_some()
    }
}

impl ValidatedProjectSettings {
    fn from_model(project: &ProjectModel) -> Result<Self> {
        let settings = Self::decode_model(project)?;
        settings.validate()?;
        Ok(settings)
    }

    fn decode_model(project: &ProjectModel) -> Result<Self> {
        Ok(Self {
            workspace_mode: WorkspaceMode::from_str(&project.workspace_mode)
                .context("project has invalid workspace mode")?,
            max_code_edit_agents: project.max_code_edit_agents,
            max_read_only_agents: project.max_read_only_agents,
            create_pr: project.create_pr,
            auto_commit: project.auto_commit,
            commit_standard: project.commit_standard.clone(),
            revert_strategy: RevertStrategy::from_str(&project.revert_strategy)
                .context("project has invalid revert strategy")?,
            stale_claim_minutes: project.stale_claim_minutes,
            worktree_cleanup_policy: WorktreeCleanupPolicy::from_str(
                &project.worktree_cleanup_policy,
            )
            .context("project has invalid worktree cleanup policy")?,
            default_agent_tool: AgentToolName::from_str(&project.default_agent_tool)
                .context("project has invalid default agent tool")?,
            default_agent_model: normalize_optional(project.default_agent_model.clone()),
            default_agent_reasoning_effort: project
                .default_agent_reasoning_effort
                .as_deref()
                .map(str::parse::<AgentReasoningEffort>)
                .transpose()
                .context("project has invalid default agent reasoning effort")?,
            agent_sandbox_mode: AgentSandboxMode::from_str(&project.agent_sandbox_mode)
                .context("project has invalid agent sandbox mode")?,
            agent_extra_writable_roots: parse_agent_extra_writable_roots_storage(
                &project.agent_extra_writable_roots,
            )
            .context("project has invalid extra writable roots")?,
            agent_git_command_policy: parse_agent_git_command_policy_storage(
                &project.agent_git_command_policy,
            )
            .context("project has invalid agent Git command policy")?,
        })
    }

    fn with_update(
        update: UpdateProjectSettings,
        existing: &ProjectModel,
        database_path: &Path,
    ) -> Result<Self> {
        if !update.has_any_field() {
            bail!("project settings update requires at least one field");
        }

        let mut settings = Self::decode_model(existing)
            .context_with(|| format!("project '{}' has invalid settings", existing.name))?;

        if let Some(workspace_mode) = update.workspace_mode {
            settings.workspace_mode = workspace_mode;
        }
        if let Some(max_code_edit_agents) = update.max_code_edit_agents {
            settings.max_code_edit_agents = max_code_edit_agents;
        }
        if let Some(max_read_only_agents) = update.max_read_only_agents {
            settings.max_read_only_agents = max_read_only_agents;
        }
        if let Some(create_pr) = update.create_pr {
            settings.create_pr = create_pr;
        }
        if let Some(auto_commit) = update.auto_commit {
            settings.auto_commit = auto_commit;
        }
        if let Some(commit_standard) = update.commit_standard {
            settings.commit_standard = commit_standard.trim().to_owned();
        }
        if let Some(revert_strategy) = update.revert_strategy {
            settings.revert_strategy = revert_strategy;
        }
        if let Some(stale_claim_minutes) = update.stale_claim_minutes {
            settings.stale_claim_minutes = stale_claim_minutes;
        }
        if let Some(worktree_cleanup_policy) = update.worktree_cleanup_policy {
            settings.worktree_cleanup_policy = worktree_cleanup_policy;
        }
        if let Some(default_agent_tool) = update.default_agent_tool {
            settings.default_agent_tool = default_agent_tool;
        }
        if let Some(default_agent_model) = update.default_agent_model {
            settings.default_agent_model = normalize_optional(default_agent_model);
        }
        if let Some(default_agent_reasoning_effort) = update.default_agent_reasoning_effort {
            settings.default_agent_reasoning_effort = default_agent_reasoning_effort;
        }
        if let Some(agent_sandbox_mode) = update.agent_sandbox_mode {
            settings.agent_sandbox_mode = agent_sandbox_mode;
        }
        if let Some(agent_extra_writable_roots) = update.agent_extra_writable_roots {
            settings.agent_extra_writable_roots =
                normalize_agent_extra_writable_roots(agent_extra_writable_roots)?;
        }
        if let Some(agent_git_command_policy) = update.agent_git_command_policy {
            settings.agent_git_command_policy = agent_git_command_policy;
        }

        validate_agent_extra_writable_roots_do_not_include_database(
            &settings.agent_extra_writable_roots,
            database_path,
        )?;
        settings.validate()?;

        Ok(settings)
    }

    fn validate(&self) -> Result<()> {
        validate_settings(
            self.workspace_mode,
            self.max_code_edit_agents,
            self.max_read_only_agents,
            self.create_pr,
            self.stale_claim_minutes,
            self.default_agent_model.as_deref(),
            self.default_agent_reasoning_effort,
        )
    }

    fn apply_to(self, project: ProjectModel) -> ProjectActiveModel {
        let mut active: ProjectActiveModel = project.into();
        active.workspace_mode = Set(self.workspace_mode.as_storage().to_owned());
        active.max_code_edit_agents = Set(self.max_code_edit_agents);
        active.max_read_only_agents = Set(self.max_read_only_agents);
        active.create_pr = Set(self.create_pr);
        active.auto_commit = Set(self.auto_commit);
        active.commit_standard = Set(self.commit_standard);
        active.revert_strategy = Set(self.revert_strategy.as_storage().to_owned());
        active.stale_claim_minutes = Set(self.stale_claim_minutes);
        active.worktree_cleanup_policy = Set(self.worktree_cleanup_policy.as_storage().to_owned());
        active.default_agent_tool = Set(self.default_agent_tool.as_storage().to_owned());
        active.default_agent_model = Set(self.default_agent_model);
        active.default_agent_reasoning_effort = Set(self
            .default_agent_reasoning_effort
            .map(|effort| effort.as_storage().to_owned()));
        active.agent_sandbox_mode = Set(self.agent_sandbox_mode.as_storage().to_owned());
        active.agent_extra_writable_roots = Set(serialize_agent_extra_writable_roots(
            &self.agent_extra_writable_roots,
        ));
        active.agent_git_command_policy = Set(serialize_agent_git_command_policy(
            &self.agent_git_command_policy,
        ));
        active.updated_at = Set(utc_now());
        active
    }
}

fn project_to_view(project: ProjectModel) -> Result<ProjectView> {
    let settings = ValidatedProjectSettings::from_model(&project)
        .context_with(|| format!("project '{}' has invalid settings", project.name))?;
    let git_status = inspect_project_git_status(project.path.as_deref(), project.path_exists);
    Ok(ProjectView {
        id: project.id,
        name: project.name,
        display_name: project.display_name,
        path: project.path,
        path_exists: project.path_exists,
        path_checked_at: project.path_checked_at,
        git_status,
        system_prompt: project.system_prompt,
        memory: project.memory,
        workspace_mode: settings.workspace_mode,
        max_code_edit_agents: settings.max_code_edit_agents,
        max_read_only_agents: settings.max_read_only_agents,
        create_pr: settings.create_pr,
        auto_commit: settings.auto_commit,
        commit_standard: settings.commit_standard,
        revert_strategy: settings.revert_strategy,
        stale_claim_minutes: settings.stale_claim_minutes,
        worktree_cleanup_policy: settings.worktree_cleanup_policy,
        default_agent_tool: settings.default_agent_tool,
        default_agent_model: settings.default_agent_model,
        default_agent_reasoning_effort: settings.default_agent_reasoning_effort,
        agent_sandbox_mode: settings.agent_sandbox_mode,
        agent_extra_writable_roots: settings.agent_extra_writable_roots,
        agent_git_command_policy: settings.agent_git_command_policy,
        created_at: project.created_at,
        updated_at: project.updated_at,
    })
}

pub async fn list_projects(store: &Store) -> Result<Vec<ProjectView>> {
    let projects = Project::find()
        .order_by_asc(project::Column::Name)
        .all(store.db().as_ref())
        .await
        .context("failed to list projects")?;

    projects.into_iter().map(project_to_view).collect()
}

pub async fn create_project(store: &Store, create: CreateProject) -> Result<ProjectView> {
    validate_project_name(&create.name)?;

    let display_name = create
        .display_name
        .unwrap_or_else(|| create.name.clone())
        .trim()
        .to_owned();
    if display_name.is_empty() {
        bail!("project display name cannot be empty");
    }
    let path = normalize_project_path(create.path)?;
    let path_exists = project_path_exists(Some(&path));
    let default_agent_model = normalize_optional(create.default_agent_model)
        .or_else(|| Some(CodexAgentModel::newest().as_storage().to_owned()));
    validate_agent_model(default_agent_model.as_deref())?;
    let default_agent_reasoning_effort = create
        .default_agent_reasoning_effort
        .unwrap_or_else(|| default_reasoning_effort_for_model(default_agent_model.as_deref()));
    validate_agent_model_reasoning_effort(
        "default agent model",
        default_agent_model.as_deref(),
        "default agent reasoning effort",
        Some(default_agent_reasoning_effort),
    )?;
    let system_prompt = create.system_prompt.unwrap_or_default();
    let memory = create.memory.unwrap_or_default();

    let now = utc_now();
    let active = ProjectActiveModel {
        name: Set(create.name),
        display_name: Set(display_name),
        path: Set(Some(path)),
        path_exists: Set(path_exists),
        path_checked_at: Set(Some(now.clone())),
        system_prompt: Set(system_prompt),
        memory: Set(memory),
        workspace_mode: Set(WorkspaceMode::CurrentBranch.as_storage().to_owned()),
        max_code_edit_agents: Set(1),
        max_read_only_agents: Set(2),
        create_pr: Set(false),
        auto_commit: Set(true),
        commit_standard: Set(String::new()),
        revert_strategy: Set(RevertStrategy::Manual.as_storage().to_owned()),
        stale_claim_minutes: Set(0),
        worktree_cleanup_policy: Set(WorktreeCleanupPolicy::Manual.as_storage().to_owned()),
        default_agent_tool: Set(AgentToolName::Codex.as_storage().to_owned()),
        default_agent_model: Set(default_agent_model),
        default_agent_reasoning_effort: Set(Some(
            default_agent_reasoning_effort.as_storage().to_owned(),
        )),
        agent_sandbox_mode: Set(AgentSandboxMode::WorkspaceWrite.as_storage().to_owned()),
        agent_extra_writable_roots: Set(String::new()),
        agent_git_command_policy: Set(default_agent_git_command_policy_json()),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    };

    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start project create")?;
    let project = active
        .insert(&txn)
        .await
        .context("failed to create project")?;
    personalities::ensure_default_personality_in_conn(&txn, project.id).await?;
    work_item_states::ensure_default_work_item_states_in_conn(&txn, project.id).await?;
    swim_lanes::ensure_default_swim_lanes_in_conn(&txn, project.id).await?;
    automation_triggers::ensure_default_project_automations_in_conn(
        &txn,
        project.id,
        &project.default_agent_tool,
    )
    .await?;
    if !project.system_prompt.trim().is_empty() {
        change_events::record_system_prompt_changed_in_tx(
            &txn,
            &project,
            "initial",
            &ProjectChangeSource::System,
        )
        .await?;
    }
    if !project.memory.trim().is_empty() {
        change_events::record_memory_changed_in_tx(
            &txn,
            &project,
            "initial",
            &ProjectChangeSource::System,
        )
        .await?;
    }
    txn.commit()
        .await
        .context("failed to commit project create")?;

    let view = project_to_view(project)?;
    events::publish_project_list_changed();
    events::publish_project_changed(&view.name);
    Ok(view)
}

pub async fn get_project(store: &Store, name: &str) -> Result<ProjectView> {
    let project = find_project_by_name(store, name).await?;
    project_to_view(project)
}

pub async fn update_project(
    store: &Store,
    name: &str,
    update: UpdateProject,
) -> Result<ProjectView> {
    if update.display_name.is_none() && update.path.is_none() {
        bail!("project update requires at least one field");
    }

    let existing = find_project_by_name(store, name).await?;
    let display_name = update
        .display_name
        .unwrap_or_else(|| existing.display_name.clone())
        .trim()
        .to_owned();
    if display_name.is_empty() {
        bail!("project display name cannot be empty");
    }

    let path = if let Some(path) = update.path {
        Some(normalize_project_path(path)?)
    } else {
        existing.path.clone()
    };
    let path_was_updated = path.as_deref() != existing.path.as_deref();
    let now = utc_now();

    let mut active: ProjectActiveModel = existing.into();
    active.display_name = Set(display_name);
    if path_was_updated {
        active.path_exists = Set(project_path_exists(path.as_deref()));
        active.path_checked_at = Set(Some(now.clone()));
    }
    active.path = Set(path);
    active.updated_at = Set(now);

    let updated = active
        .update(store.db().as_ref())
        .await
        .context_with(|| format!("failed to update project '{name}'"))?;
    let view = project_to_view(updated)?;
    events::publish_project_list_changed();
    events::publish_project_changed(&view.name);
    Ok(view)
}

pub async fn update_system_prompt(store: &Store, name: &str, body: String) -> Result<ProjectView> {
    Ok(
        update_system_prompt_with_source(store, name, body, ProjectChangeSource::User)
            .await?
            .project,
    )
}

pub async fn update_system_prompt_with_source(
    store: &Store,
    name: &str,
    body: String,
    source: ProjectChangeSource,
) -> Result<ProjectSystemPromptUpdateView> {
    let existing = find_project_by_name(store, name).await?;
    let now = utc_now();
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start project system prompt update")?;

    let mut active: ProjectActiveModel = existing.into();
    active.system_prompt = Set(body);
    active.updated_at = Set(now);

    let updated = active
        .update(&txn)
        .await
        .context_with(|| format!("failed to update system prompt for project '{name}'"))?;
    let event =
        change_events::record_system_prompt_changed_in_tx(&txn, &updated, "set", &source).await?;
    txn.commit()
        .await
        .context("failed to commit project system prompt update")?;
    events::publish_system_prompt_changed(name);

    Ok(ProjectSystemPromptUpdateView {
        project: project_to_view(updated.clone())?,
        event: change_events::system_prompt_event_to_view(name, event),
    })
}

pub async fn list_system_prompt_events(
    store: &Store,
    project_name: &str,
) -> Result<Vec<ProjectSystemPromptEventView>> {
    let project = find_project_by_name(store, project_name).await?;
    change_events::list_system_prompt_events(store.db().as_ref(), project.id, project_name).await
}

pub async fn compact_system_prompt_events(
    store: &Store,
    project_name: &str,
) -> Result<ProjectSystemPromptCompactionView> {
    let project_id = project_id(store, project_name).await?;
    let deleted =
        change_events::compact_system_prompt_events(store.db().as_ref(), project_id).await?;
    events::publish_system_prompt_changed(project_name);
    Ok(ProjectSystemPromptCompactionView {
        project_id,
        project_name: project_name.to_owned(),
        deleted_events: deleted,
    })
}

pub async fn update_memory_with_source(
    store: &Store,
    name: &str,
    body: String,
    source: ProjectChangeSource,
) -> Result<ProjectMemoryUpdateView> {
    change_memory(store, name, body, "set", source).await
}

pub async fn append_memory_with_source(
    store: &Store,
    name: &str,
    body: String,
    source: ProjectChangeSource,
) -> Result<ProjectMemoryUpdateView> {
    if body.trim().is_empty() {
        bail!("project memory append body cannot be empty");
    }

    change_memory(store, name, body, "append", source).await
}

pub async fn get_memory(store: &Store, name: &str) -> Result<ProjectMemoryView> {
    let existing = find_project_by_name(store, name).await?;
    let last_event = change_events::latest_memory_event(store.db().as_ref(), existing.id)
        .await?
        .map(|event| change_events::memory_event_to_view(name, event));
    Ok(ProjectMemoryView {
        project_id: existing.id,
        project_name: existing.name,
        memory: existing.memory,
        last_event,
        updated_at: existing.updated_at,
    })
}

pub async fn list_memory_events(
    store: &Store,
    project_name: &str,
) -> Result<Vec<ProjectMemoryEventView>> {
    let project = find_project_by_name(store, project_name).await?;
    change_events::list_memory_events(store.db().as_ref(), project.id, project_name).await
}

pub async fn compact_memory_events(
    store: &Store,
    project_name: &str,
) -> Result<ProjectMemoryCompactionView> {
    let project_id = project_id(store, project_name).await?;
    let deleted = change_events::compact_memory_events(store.db().as_ref(), project_id).await?;
    events::publish_memory_changed(project_name);
    Ok(ProjectMemoryCompactionView {
        project_id,
        project_name: project_name.to_owned(),
        deleted_events: deleted,
    })
}

pub async fn latest_memory_event_id(store: &Store, project_id: i64) -> Result<Option<i64>> {
    Ok(
        change_events::latest_memory_event(store.db().as_ref(), project_id)
            .await?
            .map(|event| event.id),
    )
}

pub async fn latest_system_prompt_event_id(store: &Store, project_id: i64) -> Result<Option<i64>> {
    Ok(
        change_events::latest_system_prompt_event(store.db().as_ref(), project_id)
            .await?
            .map(|event| event.id),
    )
}

pub async fn snapshot_current_memory_event(
    store: &Store,
    project_name: &str,
    operation: &str,
    source: ProjectChangeSource,
) -> Result<ProjectMemoryEventView> {
    let project = find_project_by_name(store, project_name).await?;
    let db = store.db();
    let event =
        change_events::record_memory_changed_in_tx(db.as_ref(), &project, operation, &source)
            .await?;
    events::publish_memory_changed(project_name);
    Ok(change_events::memory_event_to_view(project_name, event))
}

pub async fn memory_event_exists(
    store: &Store,
    project_id: i64,
    event_id: i64,
) -> Result<Option<String>> {
    change_events::memory_event_exists(store.db().as_ref(), project_id, event_id).await
}

async fn change_memory(
    store: &Store,
    name: &str,
    body: String,
    operation: &str,
    source: ProjectChangeSource,
) -> Result<ProjectMemoryUpdateView> {
    let existing = find_project_by_name(store, name).await?;
    let memory = if operation == "append" && !existing.memory.trim().is_empty() {
        format!("{}\n\n{}", existing.memory, body)
    } else {
        body
    };
    let now = utc_now();
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start project memory update")?;

    let mut active: ProjectActiveModel = existing.into();
    active.memory = Set(memory);
    active.updated_at = Set(now);

    let updated = active
        .update(&txn)
        .await
        .context_with(|| format!("failed to update memory for project '{name}'"))?;
    let event =
        change_events::record_memory_changed_in_tx(&txn, &updated, operation, &source).await?;
    txn.commit()
        .await
        .context("failed to commit project memory update")?;
    events::publish_memory_changed(name);

    Ok(ProjectMemoryUpdateView {
        project: project_to_view(updated.clone())?,
        event: change_events::memory_event_to_view(name, event),
    })
}

pub async fn get_settings(store: &Store, project_name: &str) -> Result<ProjectSettingsView> {
    let project = find_project_by_name(store, project_name).await?;
    project_settings_to_view(project)
}

pub(crate) async fn get_settings_by_id(
    store: &Store,
    project_id: i64,
) -> Result<ProjectSettingsView> {
    let project = Project::find_by_id(project_id)
        .one(store.db().as_ref())
        .await
        .context_with(|| format!("failed to load project {project_id}"))?
        .ok_or_else(|| report!("project {project_id} does not exist"))?;
    project_settings_to_view(project)
}

pub async fn update_settings(
    store: &Store,
    project_name: &str,
    update: UpdateProjectSettings,
) -> Result<ProjectSettingsView> {
    let existing = find_project_by_name(store, project_name).await?;
    let active =
        ValidatedProjectSettings::with_update(update, &existing, store.path())?.apply_to(existing);

    let updated = active
        .update(store.db().as_ref())
        .await
        .context_with(|| format!("failed to update settings for project '{project_name}'"))?;
    let settings = project_settings_to_view(updated)?;
    events::publish_project_changed(project_name);
    Ok(settings)
}

pub fn allowed_code_edit_agents(settings: &ProjectSettingsView) -> i64 {
    if settings.workspace_mode == WorkspaceMode::GitWorktree {
        settings.max_code_edit_agents
    } else {
        settings.max_code_edit_agents.min(1)
    }
}

pub async fn delete_project(store: &Store, name: &str) -> Result<()> {
    let project = find_project_by_name(store, name).await?;
    Project::delete_by_id(project.id)
        .exec(store.db().as_ref())
        .await
        .context_with(|| format!("failed to delete project '{name}'"))?;
    events::publish_project_list_changed();
    Ok(())
}

pub fn spawn_path_status_checker_until(
    store: Store,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PROJECT_PATH_CHECK_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(err) = refresh_project_path_statuses(&store).await {
                        tracing::warn!(error = %format_args!("{err:#}"), "project path status check failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

pub async fn refresh_project_path_statuses(store: &Store) -> Result<Vec<ProjectView>> {
    let projects = Project::find()
        .order_by_asc(project::Column::Name)
        .all(store.db().as_ref())
        .await
        .context("failed to list projects for path status check")?;
    let checked_at = utc_now();
    let mut refreshed = Vec::with_capacity(projects.len());
    for project in projects {
        refreshed.push(update_project_path_status(store, project, checked_at.clone()).await?);
    }
    Ok(refreshed)
}

pub(crate) async fn refresh_project_path_status(
    store: &Store,
    project_id: i64,
) -> Result<ProjectView> {
    let project = Project::find_by_id(project_id)
        .one(store.db().as_ref())
        .await
        .context_with(|| format!("failed to load project {project_id} for path status check"))?
        .ok_or_else(|| report!("project {project_id} does not exist"))?;
    update_project_path_status(store, project, utc_now()).await
}

pub(crate) async fn project_id(store: &Store, name: &str) -> Result<i64> {
    Ok(find_project_by_name(store, name).await?.id)
}

pub(crate) async fn project_name_by_id(store: &Store, project_id: i64) -> Result<String> {
    Ok(Project::find_by_id(project_id)
        .one(store.db().as_ref())
        .await
        .context_with(|| format!("failed to load project {project_id}"))?
        .ok_or_else(|| report!("project {project_id} does not exist"))?
        .name)
}

pub(crate) async fn find_project_by_name(store: &Store, name: &str) -> Result<ProjectModel> {
    Project::find()
        .filter(project::Column::Name.eq(name))
        .one(store.db().as_ref())
        .await
        .context_with(|| format!("failed to load project '{name}'"))?
        .ok_or_else(|| report!("project '{name}' does not exist"))
}

fn validate_project_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("project name cannot be empty");
    }

    if name.trim() != name {
        bail!("project name cannot have leading or trailing whitespace");
    }

    if name.contains('/') || name.contains('\\') {
        bail!("project name cannot contain path separators");
    }

    Ok(())
}

pub(crate) fn normalize_project_path(path: PathBuf) -> Result<String> {
    let path = expand_home_path(&path.to_string_lossy());
    if path.is_empty() {
        bail!("project path is required");
    }

    if !PathBuf::from(&path).is_dir() {
        bail!("project path '{path}' is not a directory");
    }

    Ok(path)
}

fn project_path_exists(path: Option<&str>) -> bool {
    path.map(expand_home_path)
        .is_some_and(|path| Path::new(&path).is_dir())
}

fn inspect_project_git_status(
    path: Option<&str>,
    path_exists: bool,
) -> Option<ProjectGitStatusView> {
    let path = path.map(str::trim).filter(|path| !path.is_empty())?;
    if !path_exists {
        return None;
    }

    let expanded = expand_home_path(path);
    let repository = match Repository::discover(Path::new(&expanded)) {
        Ok(repository) => repository,
        Err(err) if err.code() == GitErrorCode::NotFound => {
            return Some(ProjectGitStatusView {
                is_repository: false,
                branch: None,
                added_lines: 0,
                deleted_lines: 0,
                error: None,
            });
        }
        Err(err) => {
            return Some(ProjectGitStatusView {
                is_repository: false,
                branch: None,
                added_lines: 0,
                deleted_lines: 0,
                error: Some(err.message().to_owned()),
            });
        }
    };

    let branch = current_git_branch(&repository);
    let (added_lines, deleted_lines, error) = match git_diff_line_counts(&repository) {
        Ok((added_lines, deleted_lines)) => (added_lines, deleted_lines, None),
        Err(err) => (0, 0, Some(err.to_string())),
    };

    Some(ProjectGitStatusView {
        is_repository: true,
        branch,
        added_lines,
        deleted_lines,
        error,
    })
}

fn current_git_branch(repository: &Repository) -> Option<String> {
    let head = repository.head().ok()?;
    if head.is_branch() {
        return head.shorthand().map(ToOwned::to_owned);
    }
    head.target()
        .map(|oid| format!("detached {}", short_oid(oid)))
}

fn short_oid(oid: Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

fn git_diff_line_counts(repository: &Repository) -> Result<(u64, u64)> {
    let tree = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    let mut diff_options = DiffOptions::new();
    diff_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .ignore_submodules(true);
    let diff = repository
        .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut diff_options))
        .context("failed to diff project git workspace")?;
    let stats = diff
        .stats()
        .context("failed to summarize project git diff")?;
    Ok((stats.insertions() as u64, stats.deletions() as u64))
}

fn expand_home_path(path: &str) -> String {
    expand_home_path_with(path.trim(), env::var_os("HOME").as_ref())
}

fn expand_home_path_with(path: &str, home: Option<&std::ffi::OsString>) -> String {
    if path == "~" {
        return home
            .map(|home| home.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_owned());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home
    {
        return PathBuf::from(home)
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    path.to_owned()
}

async fn update_project_path_status(
    store: &Store,
    project: ProjectModel,
    checked_at: String,
) -> Result<ProjectView> {
    let exists = project_path_exists(project.path.as_deref());
    let mut active: ProjectActiveModel = project.into();
    active.path_exists = Set(exists);
    active.path_checked_at = Set(Some(checked_at));
    let updated = active
        .update(store.db().as_ref())
        .await
        .context("failed to update project path status")?;
    project_to_view(updated)
}

pub(crate) fn validate_settings(
    workspace_mode: WorkspaceMode,
    max_code_edit_agents: i64,
    max_read_only_agents: i64,
    create_pr: bool,
    stale_claim_minutes: i64,
    default_agent_model: Option<&str>,
    default_agent_reasoning_effort: Option<AgentReasoningEffort>,
) -> Result<()> {
    if max_code_edit_agents < 1 {
        bail!("max code-editing agents must be at least 1");
    }
    if max_code_edit_agents > 1 && workspace_mode != WorkspaceMode::GitWorktree {
        bail!("only git_worktree strategy can run multiple agents in parallel");
    }
    if max_read_only_agents < 0 {
        bail!("max read-only agents cannot be negative");
    }
    if create_pr && workspace_mode == WorkspaceMode::CurrentBranch {
        bail!("pull requests can only be created for git_worktree or git_branch strategies");
    }
    if stale_claim_minutes < 0 {
        bail!("stale claim minutes cannot be negative");
    }
    validate_agent_model(default_agent_model)?;
    validate_agent_model_reasoning_effort(
        "default agent model",
        default_agent_model,
        "default agent reasoning effort",
        default_agent_reasoning_effort,
    )?;
    Ok(())
}

pub(crate) fn validate_agent_model(default_agent_model: Option<&str>) -> Result<()> {
    validate_agent_model_field("default agent model", default_agent_model)
}

pub(crate) fn validate_agent_model_field(label: &str, model: Option<&str>) -> Result<()> {
    if let Some(model) = model {
        if model.trim().is_empty() {
            bail!("{label} cannot be empty");
        }
        if !CodexAgentModel::is_available_model(model) {
            bail!(
                "{label} must be one of: {}",
                CodexAgentModel::allowed_values()
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_agent_model_reasoning_effort(
    model_label: &str,
    model: Option<&str>,
    effort_label: &str,
    effort: Option<AgentReasoningEffort>,
) -> Result<()> {
    let (Some(model), Some(effort)) = (model, effort) else {
        return Ok(());
    };
    let model = model.parse::<CodexAgentModel>().context_with(|| {
        format!(
            "{model_label} must be one of: {}",
            CodexAgentModel::allowed_values()
        )
    })?;
    if !model.supports_reasoning_effort(effort) {
        bail!(
            "{model_label} '{}' is incompatible with {effort_label} '{}'; supported efforts are: {}",
            model.as_storage(),
            effort.as_storage(),
            model.allowed_reasoning_effort_values()
        );
    }
    Ok(())
}

pub(crate) fn default_reasoning_effort_for_model(model: Option<&str>) -> AgentReasoningEffort {
    model
        .and_then(|model| model.parse::<CodexAgentModel>().ok())
        .map(CodexAgentModel::highest_reasoning_effort)
        .unwrap_or_else(AgentReasoningEffort::highest)
}

fn project_settings_to_view(project: ProjectModel) -> Result<ProjectSettingsView> {
    let settings = ValidatedProjectSettings::from_model(&project)
        .context_with(|| format!("project '{}' has invalid settings", project.name))?;
    Ok(ProjectSettingsView {
        id: project.id,
        project_id: project.id,
        workspace_mode: settings.workspace_mode,
        max_code_edit_agents: settings.max_code_edit_agents,
        max_read_only_agents: settings.max_read_only_agents,
        create_pr: settings.create_pr,
        auto_commit: settings.auto_commit,
        commit_standard: settings.commit_standard,
        revert_strategy: settings.revert_strategy,
        stale_claim_minutes: settings.stale_claim_minutes,
        worktree_cleanup_policy: settings.worktree_cleanup_policy,
        default_agent_tool: settings.default_agent_tool,
        default_agent_model: settings.default_agent_model,
        default_agent_reasoning_effort: settings.default_agent_reasoning_effort,
        agent_sandbox_mode: settings.agent_sandbox_mode,
        agent_extra_writable_roots: settings.agent_extra_writable_roots,
        agent_git_command_policy: settings.agent_git_command_policy,
        created_at: project.created_at,
        updated_at: project.updated_at,
    })
}

pub(crate) fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

pub(crate) fn parse_agent_extra_writable_roots_text(value: &str) -> Result<Vec<String>> {
    normalize_agent_extra_writable_roots(value.lines().map(str::to_owned).collect())
}

pub(crate) fn parse_agent_extra_writable_roots_storage(value: &str) -> Result<Vec<String>> {
    parse_agent_extra_writable_roots_text(value)
}

pub(crate) fn serialize_agent_extra_writable_roots(roots: &[String]) -> String {
    roots.join("\n")
}

pub(crate) fn default_agent_git_command_policy_json() -> String {
    serialize_agent_git_command_policy(&AgentGitCommandPolicy::default())
}

pub(crate) fn parse_agent_git_command_policy_storage(value: &str) -> Result<AgentGitCommandPolicy> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(AgentGitCommandPolicy::default());
    }
    Ok(serde_json::from_str(value).context("failed to parse agent git command policy")?)
}

pub(crate) fn serialize_agent_git_command_policy(policy: &AgentGitCommandPolicy) -> String {
    serde_json::to_string(policy).expect("agent git command policy must serialize")
}

pub(crate) fn normalize_agent_extra_writable_roots(roots: Vec<String>) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for root in roots {
        let root = root.trim();
        if root.is_empty() {
            continue;
        }
        let expanded = expand_home_path(root);
        if !Path::new(&expanded).is_absolute() {
            bail!("agent extra writable root '{root}' must resolve to an absolute path");
        }
        if seen.insert(expanded.clone()) {
            normalized.push(expanded);
        }
    }
    Ok(normalized)
}

pub(crate) fn validate_agent_extra_writable_roots_do_not_include_database(
    roots: &[String],
    database_path: &Path,
) -> Result<()> {
    for root in roots {
        let root_path = Path::new(root);
        if database_path.starts_with(root_path) {
            bail!(
                "agent extra writable root '{}' includes Dispatch database {}; choose a narrower directory",
                root,
                database_path.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::Signature;
    use sea_orm::ActiveValue::Set;
    use sea_orm::EntityTrait;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::entities::work_item_event;

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        (temp, store)
    }

    fn project_path(temp: &TempDir, name: &str) -> PathBuf {
        let path = temp.path().join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn commit_all(repository: &Repository, message: &str) {
        let mut index = repository.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = Signature::now("Dispatch Test", "dispatch@example.com").unwrap();
        repository
            .commit(
                Some("refs/heads/main"),
                &signature,
                &signature,
                message,
                &tree,
                &[],
            )
            .unwrap();
        repository.set_head("refs/heads/main").unwrap();
    }

    fn project_model(path: PathBuf) -> ProjectModel {
        ProjectModel {
            id: 1,
            name: "demo".to_owned(),
            display_name: "Demo".to_owned(),
            path: Some(path.to_string_lossy().into_owned()),
            path_exists: true,
            path_checked_at: Some("2026-06-19T00:00:00Z".to_owned()),
            system_prompt: String::new(),
            memory: String::new(),
            workspace_mode: WorkspaceMode::CurrentBranch.as_storage().to_owned(),
            max_code_edit_agents: 1,
            max_read_only_agents: 2,
            create_pr: false,
            auto_commit: true,
            commit_standard: String::new(),
            revert_strategy: RevertStrategy::Manual.as_storage().to_owned(),
            stale_claim_minutes: 0,
            worktree_cleanup_policy: WorktreeCleanupPolicy::Manual.as_storage().to_owned(),
            default_agent_tool: AgentToolName::Codex.as_storage().to_owned(),
            default_agent_model: Some(CodexAgentModel::newest().as_storage().to_owned()),
            default_agent_reasoning_effort: Some(
                AgentReasoningEffort::highest().as_storage().to_owned(),
            ),
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite.as_storage().to_owned(),
            agent_extra_writable_roots: String::new(),
            agent_git_command_policy: default_agent_git_command_policy_json(),
            created_at: "2026-06-19T00:00:00Z".to_owned(),
            updated_at: "2026-06-19T00:00:00Z".to_owned(),
        }
    }

    async fn create_demo_project(store: &Store, path: PathBuf) {
        create_project(
            store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path,
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
    }

    #[test]
    fn settings_plan_merges_validates_and_applies_updates() {
        let temp = TempDir::new().unwrap();
        let project = project_model(project_path(&temp, "demo"));
        let database_path = temp.path().join("dispatch.sqlite3");

        let plan = ValidatedProjectSettings::with_update(
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                max_read_only_agents: Some(0),
                create_pr: Some(true),
                commit_standard: Some(" Use short subjects. ".to_owned()),
                default_agent_model: Some(Some("  ".to_owned())),
                agent_extra_writable_roots: Some(vec![
                    " ~/Library/Caches/chrome-for-testing-manager ".to_owned(),
                    "~/Library/Caches/chrome-for-testing-manager".to_owned(),
                    "/tmp/dispatch-browser".to_owned(),
                ]),
                agent_git_command_policy: Some(AgentGitCommandPolicy {
                    add: true,
                    commit: false,
                    push: true,
                    reset: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            &project,
            &database_path,
        )
        .unwrap();

        assert_eq!(plan.workspace_mode, WorkspaceMode::GitBranch);
        assert_eq!(plan.max_code_edit_agents, 1);
        assert_eq!(plan.max_read_only_agents, 0);
        assert!(plan.create_pr);
        assert_eq!(plan.commit_standard, "Use short subjects.");
        assert_eq!(plan.default_agent_model, None);
        assert_eq!(
            plan.agent_extra_writable_roots,
            vec![
                expand_home_path("~/Library/Caches/chrome-for-testing-manager"),
                "/tmp/dispatch-browser".to_owned(),
            ]
        );

        let active = plan.apply_to(project);

        assert_eq!(
            active.workspace_mode,
            Set(WorkspaceMode::GitBranch.as_storage().to_owned())
        );
        assert_eq!(active.max_read_only_agents, Set(0));
        assert_eq!(active.create_pr, Set(true));
        assert_eq!(
            active.commit_standard,
            Set("Use short subjects.".to_owned())
        );
        assert_eq!(active.default_agent_model, Set(None));
    }

    #[test]
    fn settings_plan_rejects_empty_update() {
        let temp = TempDir::new().unwrap();
        let project = project_model(project_path(&temp, "demo"));
        let database_path = temp.path().join("dispatch.sqlite3");

        let err = ValidatedProjectSettings::with_update(
            UpdateProjectSettings::default(),
            &project,
            &database_path,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("project settings update requires at least one field")
        );
    }

    #[test]
    fn project_view_reports_invalid_persisted_settings_instead_of_panicking() {
        let temp = TempDir::new().unwrap();
        let mut project = project_model(project_path(&temp, "demo"));
        project.workspace_mode = "parallel_everywhere".to_owned();

        let err = project_to_view(project).unwrap_err();

        assert!(format!("{err:#}").contains("project has invalid workspace mode"));
    }

    #[test]
    fn project_settings_reject_invalid_persisted_json_at_the_storage_boundary() {
        let temp = TempDir::new().unwrap();
        let mut project = project_model(project_path(&temp, "demo"));
        project.agent_git_command_policy = "{ definitely not json }".to_owned();

        let err = ValidatedProjectSettings::from_model(&project).unwrap_err();

        assert!(format!("{err:#}").contains("project has invalid agent Git command policy"));
    }

    #[tokio::test]
    async fn missing_project_is_rejected() {
        let (_temp, store) = test_store().await;

        let err = get_project(&store, "missing").await.unwrap_err();

        assert!(err.to_string().contains("project 'missing' does not exist"));
    }

    #[tokio::test]
    async fn creating_project_requires_path() {
        let (_temp, store) = test_store().await;

        let err = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: PathBuf::new(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("project path is required"));
    }

    #[test]
    fn project_path_expands_home_prefix() {
        let home = std::ffi::OsString::from("/Users/example");

        assert_eq!(
            expand_home_path_with("~/dev/vibetest", Some(&home)),
            "/Users/example/dev/vibetest"
        );
        assert_eq!(expand_home_path_with("~", Some(&home)), "/Users/example");
    }

    #[test]
    fn project_git_status_reports_branch_and_diff_counts() {
        let temp = TempDir::new().unwrap();
        let repository = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("notes.txt"), "one\ntwo\n").unwrap();
        commit_all(&repository, "Initial commit");
        fs::write(temp.path().join("notes.txt"), "one\nthree\nfour\n").unwrap();

        let status = inspect_project_git_status(Some(temp.path().to_str().unwrap()), true).unwrap();

        assert!(status.is_repository);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.added_lines, 2);
        assert_eq!(status.deleted_lines, 1);
        assert!(status.error.is_none());
    }

    #[test]
    fn project_git_status_reports_existing_non_repository() {
        let temp = TempDir::new().unwrap();

        let status = inspect_project_git_status(Some(temp.path().to_str().unwrap()), true).unwrap();

        assert!(!status.is_repository);
        assert!(status.branch.is_none());
        assert_eq!(status.added_lines, 0);
        assert_eq!(status.deleted_lines, 0);
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn project_crud_preserves_name_and_updates_path() {
        let (temp, store) = test_store().await;
        let demo_path = project_path(&temp, "demo-path");
        let new_demo_path = project_path(&temp, "new-demo-path");

        let created = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: demo_path.clone(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: Some("Prefer small changes.".to_owned()),
                memory: Some("Initial memory.".to_owned()),
            },
        )
        .await
        .unwrap();

        assert_eq!(created.name, "demo");
        assert_eq!(created.display_name, "demo");
        assert_eq!(created.path.as_deref(), Some(demo_path.to_str().unwrap()));
        assert_eq!(created.system_prompt, "Prefer small changes.");
        assert_eq!(created.memory, "Initial memory.");

        let updated = update_project(
            &store,
            "demo",
            UpdateProject {
                display_name: Some("Demo Project".to_owned()),
                path: Some(new_demo_path.clone()),
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.display_name, "Demo Project");
        assert_eq!(
            updated.path.as_deref(),
            Some(new_demo_path.to_str().unwrap())
        );
        assert_eq!(updated.system_prompt, "Prefer small changes.");
        assert_eq!(updated.memory, "Initial memory.");
    }

    #[tokio::test]
    async fn path_status_refresh_detects_deleted_path() {
        let (temp, store) = test_store().await;
        let demo_path = project_path(&temp, "demo-path");
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: demo_path.clone(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        fs::remove_dir_all(&demo_path).unwrap();

        let refreshed = refresh_project_path_statuses(&store).await.unwrap();

        assert_eq!(refreshed.len(), 1);
        assert!(!refreshed[0].path_exists);
        assert!(refreshed[0].path_checked_at.is_some());
    }

    #[tokio::test]
    async fn project_context_has_separate_update_paths() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let prompted = update_system_prompt(&store, "demo", "User-controlled prompt".to_owned())
            .await
            .unwrap();
        let remembered = append_memory_with_source(
            &store,
            "demo",
            "Shared project memory".to_owned(),
            ProjectChangeSource::User,
        )
        .await
        .unwrap()
        .project;

        assert_eq!(prompted.system_prompt, "User-controlled prompt");
        assert_eq!(remembered.memory, "Shared project memory");
    }

    #[tokio::test]
    async fn system_prompt_history_snapshots_initial_and_updated_values() {
        let (temp, store) = test_store().await;
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: project_path(&temp, "demo"),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: Some("Initial prompt.".to_owned()),
                memory: None,
            },
        )
        .await
        .unwrap();

        let initial_events = list_system_prompt_events(&store, "demo").await.unwrap();
        assert_eq!(initial_events.len(), 1);
        assert_eq!(initial_events[0].operation, "initial");
        assert_eq!(initial_events[0].system_prompt, "Initial prompt.");
        assert_eq!(initial_events[0].actor_type.as_deref(), Some("system"));

        let updated = update_system_prompt_with_source(
            &store,
            "demo",
            "Updated prompt.".to_owned(),
            ProjectChangeSource::User,
        )
        .await
        .unwrap();
        assert_eq!(updated.project.system_prompt, "Updated prompt.");
        assert_eq!(updated.event.operation, "set");
        assert_eq!(updated.event.system_prompt, "Updated prompt.");
        assert_eq!(updated.event.actor_type.as_deref(), Some("user"));

        let current = get_project(&store, "demo").await.unwrap();
        assert_eq!(current.system_prompt, "Updated prompt.");

        let events = list_system_prompt_events(&store, "demo").await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, updated.event.id);

        let compacted = compact_system_prompt_events(&store, "demo").await.unwrap();
        assert_eq!(compacted.deleted_events, 2);
        assert!(
            list_system_prompt_events(&store, "demo")
                .await
                .unwrap()
                .is_empty()
        );
        let current = get_project(&store, "demo").await.unwrap();
        assert_eq!(current.system_prompt, "Updated prompt.");
    }

    #[tokio::test]
    async fn project_context_events_persist_project_level_attribution() {
        let (temp, store) = test_store().await;
        let project = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: project_path(&temp, "demo"),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();

        let prompted = update_system_prompt_with_source(
            &store,
            "demo",
            "Agent prompt.".to_owned(),
            ProjectChangeSource::Agent {
                agent_id: "dispatch-run-42".to_owned(),
                agent_run_id: None,
            },
        )
        .await
        .unwrap();
        let prompt_row = work_item_event::Entity::find_by_id(prompted.event.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap();
        let prompt_body =
            serde_json::from_str::<change_events::SystemPromptChangedBody>(&prompt_row.body)
                .expect("system prompt event body should decode");

        assert_eq!(prompted.event.actor_type.as_deref(), Some("agent"));
        assert_eq!(prompted.event.actor_id.as_deref(), Some("dispatch-run-42"));
        assert_eq!(prompted.event.agent_run_id, Some(42));
        assert_eq!(prompt_row.project_id, project.id);
        assert_eq!(prompt_row.work_item_id, None);
        assert_eq!(
            prompt_row.event_type,
            change_events::SYSTEM_PROMPT_CHANGED_EVENT_TYPE.as_storage()
        );
        assert_eq!(prompt_row.actor_type.as_deref(), Some("agent"));
        assert_eq!(prompt_row.actor_id.as_deref(), Some("dispatch-run-42"));
        assert_eq!(prompt_row.agent_run_id, Some(42));
        assert_eq!(prompt_body.operation, "set");
        assert_eq!(prompt_body.system_prompt, "Agent prompt.");

        let remembered = append_memory_with_source(
            &store,
            "demo",
            "Agent memory.".to_owned(),
            ProjectChangeSource::Agent {
                agent_id: "codex-worker".to_owned(),
                agent_run_id: Some(77),
            },
        )
        .await
        .unwrap();
        let memory_row = work_item_event::Entity::find_by_id(remembered.event.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap();
        let memory_body =
            serde_json::from_str::<change_events::MemoryChangedBody>(&memory_row.body).unwrap();

        assert_eq!(remembered.event.actor_type.as_deref(), Some("agent"));
        assert_eq!(remembered.event.actor_id.as_deref(), Some("codex-worker"));
        assert_eq!(remembered.event.agent_run_id, Some(77));
        assert_eq!(memory_row.project_id, project.id);
        assert_eq!(memory_row.work_item_id, None);
        assert_eq!(
            memory_row.event_type,
            change_events::MEMORY_CHANGED_EVENT_TYPE.as_storage()
        );
        assert_eq!(memory_row.actor_type.as_deref(), Some("agent"));
        assert_eq!(memory_row.actor_id.as_deref(), Some("codex-worker"));
        assert_eq!(memory_row.agent_run_id, Some(77));
        assert_eq!(memory_body.operation, "append");
        assert_eq!(memory_body.memory, "Agent memory.");
    }

    #[tokio::test]
    async fn settings_are_created_with_safe_defaults() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let settings = get_settings(&store, "demo").await.unwrap();

        assert_eq!(settings.workspace_mode, WorkspaceMode::CurrentBranch);
        assert_eq!(allowed_code_edit_agents(&settings), 1);
        assert_eq!(settings.max_read_only_agents, 2);
        assert!(!settings.create_pr);
        assert!(settings.auto_commit);
        assert_eq!(settings.commit_standard, "");
        assert_eq!(settings.revert_strategy, RevertStrategy::Manual);
        assert_eq!(settings.stale_claim_minutes, 0);
        assert_eq!(
            settings.worktree_cleanup_policy,
            WorktreeCleanupPolicy::Manual
        );
        assert_eq!(settings.default_agent_tool, AgentToolName::Codex);
        assert_eq!(
            settings.default_agent_model.as_deref(),
            Some(CodexAgentModel::newest().as_storage())
        );
        assert_eq!(
            settings.default_agent_reasoning_effort,
            Some(AgentReasoningEffort::highest())
        );
        assert_eq!(
            settings.agent_sandbox_mode,
            AgentSandboxMode::WorkspaceWrite
        );
        assert!(settings.agent_extra_writable_roots.is_empty());
        assert_eq!(
            settings.agent_git_command_policy,
            AgentGitCommandPolicy::default()
        );
    }

    #[tokio::test]
    async fn project_create_accepts_known_default_agent_model() {
        let (temp, store) = test_store().await;

        let project = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: project_path(&temp, "demo"),
                default_agent_model: Some("gpt-5.4-mini".to_owned()),
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(project.default_agent_model.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(
            project.default_agent_reasoning_effort,
            Some(AgentReasoningEffort::XHigh)
        );
    }

    #[tokio::test]
    async fn project_create_accepts_default_agent_reasoning_effort() {
        let (temp, store) = test_store().await;

        let project = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: project_path(&temp, "demo"),
                default_agent_model: None,
                default_agent_reasoning_effort: Some(AgentReasoningEffort::High),
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(
            project.default_agent_reasoning_effort,
            Some(AgentReasoningEffort::High)
        );
    }

    #[tokio::test]
    async fn settings_reject_unknown_default_agent_model() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                default_agent_model: Some(Some("gpt-4.1-codex".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("default agent model must be one of")
        );
    }

    #[tokio::test]
    async fn settings_reject_incompatible_model_and_reasoning_effort() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                default_agent_model: Some(Some("gpt-5.6-sol".to_owned())),
                default_agent_reasoning_effort: Some(Some(AgentReasoningEffort::Minimal)),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("default agent model"));
        assert!(err.to_string().contains("incompatible"));
    }

    #[tokio::test]
    async fn settings_update_the_project_row() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let settings = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                max_read_only_agents: Some(4),
                create_pr: Some(true),
                auto_commit: Some(false),
                commit_standard: Some(" Use Conventional Commits. ".to_owned()),
                revert_strategy: Some(RevertStrategy::GitReset),
                default_agent_tool: Some(AgentToolName::Codex),
                agent_sandbox_mode: Some(AgentSandboxMode::DangerFullAccess),
                agent_extra_writable_roots: Some(vec![
                    " ~/Library/Caches/chrome-for-testing-manager ".to_owned(),
                    "".to_owned(),
                    "~/Library/Caches/chrome-for-testing-manager".to_owned(),
                    "/tmp/dispatch-browser".to_owned(),
                ]),
                agent_git_command_policy: Some(AgentGitCommandPolicy {
                    add: true,
                    commit: false,
                    push: true,
                    reset: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let project = get_project(&store, "demo").await.unwrap();

        assert_eq!(settings.project_id, project.id);
        assert_eq!(settings.workspace_mode, WorkspaceMode::GitBranch);
        assert_eq!(project.workspace_mode, WorkspaceMode::GitBranch);
        assert_eq!(settings.max_read_only_agents, 4);
        assert_eq!(project.max_read_only_agents, 4);
        assert!(!settings.auto_commit);
        assert!(!project.auto_commit);
        assert_eq!(settings.commit_standard, "Use Conventional Commits.");
        assert_eq!(project.commit_standard, "Use Conventional Commits.");
        assert_eq!(settings.revert_strategy, RevertStrategy::GitReset);
        assert_eq!(project.revert_strategy, RevertStrategy::GitReset);
        assert_eq!(project.default_agent_tool, AgentToolName::Codex);
        assert_eq!(
            settings.agent_sandbox_mode,
            AgentSandboxMode::DangerFullAccess
        );
        assert_eq!(
            project.agent_sandbox_mode,
            AgentSandboxMode::DangerFullAccess
        );
        assert_eq!(
            settings.agent_extra_writable_roots,
            vec![
                expand_home_path("~/Library/Caches/chrome-for-testing-manager"),
                "/tmp/dispatch-browser".to_owned(),
            ]
        );
        assert_eq!(
            project.agent_extra_writable_roots,
            settings.agent_extra_writable_roots
        );
        assert_eq!(
            settings.agent_git_command_policy,
            AgentGitCommandPolicy {
                add: true,
                commit: false,
                push: true,
                reset: false,
                ..Default::default()
            }
        );
        assert_eq!(
            project.agent_git_command_policy,
            settings.agent_git_command_policy
        );
    }

    #[tokio::test]
    async fn settings_reject_roots_that_include_database() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                agent_extra_writable_roots: Some(vec![temp.path().to_string_lossy().into_owned()]),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("includes Dispatch database"));
    }

    #[tokio::test]
    async fn settings_reject_zero_code_edit_agents() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                max_code_edit_agents: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("at least 1"));
    }

    #[tokio::test]
    async fn settings_allow_zero_read_only_agents_but_reject_negative_values() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let settings = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                max_read_only_agents: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(settings.max_read_only_agents, 0);
        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                max_read_only_agents: Some(-1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("max read-only agents"));
    }

    #[tokio::test]
    async fn non_worktree_strategy_rejects_parallel_agents() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                max_code_edit_agents: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("only git_worktree"));
    }

    #[tokio::test]
    async fn current_branch_rejects_pull_request_creation() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::CurrentBranch),
                create_pr: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("pull requests"));
    }

    #[tokio::test]
    async fn branch_strategy_allows_pull_requests_but_caps_concurrency() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let settings = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                create_pr: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(settings.create_pr);
        assert_eq!(allowed_code_edit_agents(&settings), 1);
    }

    #[tokio::test]
    async fn stale_claim_timeout_cannot_be_negative() {
        let (temp, store) = test_store().await;
        create_demo_project(&store, project_path(&temp, "demo")).await;

        let err = update_settings(
            &store,
            "demo",
            UpdateProjectSettings {
                stale_claim_minutes: Some(-1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("stale claim"));
    }
}
