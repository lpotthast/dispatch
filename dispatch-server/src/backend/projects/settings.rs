use std::{collections::BTreeSet, path::Path, str::FromStr};

use crate::backend::knowledge::normalize_knowledge_directory;
use rootcause::{Result, prelude::*};
use sea_orm::ActiveValue::Set;

use crate::{
    backend::{
        entities::project::{ProjectActiveModel, ProjectModel},
        storage::utc_now,
    },
    shared::view_models::{
        AgentGitCommandPolicy, AgentReasoningEffort, AgentSandboxMode, AgentToolName,
        CodexAgentModel, ProjectSettingsView, RevertStrategy, WorkspaceMode, WorktreeCleanupPolicy,
    },
};

use super::expand_home_path;

#[derive(Clone, Debug, Default)]
pub struct UpdateProjectSettings {
    pub knowledge_directory: Option<String>,
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
pub(super) struct ValidatedProjectSettings {
    pub(super) knowledge_directory: String,
    pub(super) workspace_mode: WorkspaceMode,
    pub(super) max_code_edit_agents: i64,
    pub(super) max_read_only_agents: i64,
    pub(super) create_pr: bool,
    pub(super) auto_commit: bool,
    pub(super) commit_standard: String,
    pub(super) revert_strategy: RevertStrategy,
    pub(super) stale_claim_minutes: i64,
    pub(super) worktree_cleanup_policy: WorktreeCleanupPolicy,
    pub(super) default_agent_tool: AgentToolName,
    pub(super) default_agent_model: Option<String>,
    pub(super) default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub(super) agent_sandbox_mode: AgentSandboxMode,
    pub(super) agent_extra_writable_roots: Vec<String>,
    pub(super) agent_git_command_policy: AgentGitCommandPolicy,
}

impl UpdateProjectSettings {
    fn has_any_field(&self) -> bool {
        self.workspace_mode.is_some()
            || self.knowledge_directory.is_some()
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
    pub(super) fn from_model(project: &ProjectModel) -> Result<Self> {
        let settings = Self::decode_model(project)?;
        settings.validate()?;
        Ok(settings)
    }

    fn decode_model(project: &ProjectModel) -> Result<Self> {
        Ok(Self {
            knowledge_directory: normalize_knowledge_directory(&project.knowledge_directory)
                .context("project has invalid knowledge directory")?,
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

    pub(super) fn with_update(
        update: UpdateProjectSettings,
        existing: &ProjectModel,
        database_path: &Path,
    ) -> Result<Self> {
        if !update.has_any_field() {
            bail!("project settings update requires at least one field");
        }

        let mut settings = Self::decode_model(existing)
            .context_with(|| format!("project '{}' has invalid settings", existing.name))?;

        if let Some(knowledge_directory) = update.knowledge_directory {
            settings.knowledge_directory =
                validate_knowledge_directory_update(existing, &knowledge_directory)?;
        }
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

    pub(super) fn apply_to(self, project: ProjectModel) -> ProjectActiveModel {
        let mut active: ProjectActiveModel = project.into();
        active.knowledge_directory = Set(self.knowledge_directory);
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

pub(super) fn project_settings_to_view(project: ProjectModel) -> Result<ProjectSettingsView> {
    let settings = ValidatedProjectSettings::from_model(&project)
        .context_with(|| format!("project '{}' has invalid settings", project.name))?;
    Ok(ProjectSettingsView {
        id: project.id,
        project_id: project.id,
        knowledge_directory: settings.knowledge_directory,
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

pub(crate) fn validate_knowledge_directory_update(
    project: &ProjectModel,
    value: &str,
) -> Result<String> {
    let knowledge_directory =
        normalize_knowledge_directory(value).context("project has invalid knowledge directory")?;
    if knowledge_directory != project.knowledge_directory
        && let Some(workspace) = &project.path
    {
        match Path::new(workspace)
            .join(&project.knowledge_directory)
            .read_dir()
        {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    bail!(
                        "cannot change the knowledge directory while it contains files; move it explicitly"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(report!(error)
                .context(
                    "cannot inspect the existing knowledge directory before changing its location",
                )
                .into_dynamic()),
        }
    }
    Ok(knowledge_directory)
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
