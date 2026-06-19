use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use codex_app_server_sdk::{ModelReasoningEffort as CodexReasoningEffort, SandboxMode};
use rootcause::{Result, prelude::*};

use crate::shared::view_models::{
    AgentGitHardResetPolicy, AgentGitRuntimePolicy, AgentReasoningEffort, AgentSandboxMode,
    AutomationRunMutability, ProjectSettingsView, WorkspaceMode,
};

pub(crate) struct GitRuntimeFiles {
    pub(crate) shim_dir: PathBuf,
    pub(crate) policy_path: PathBuf,
}

pub(crate) fn prepare_git_runtime(
    run_id: i64,
    log_dir: &Path,
    patchbay_binary: &Path,
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> Result<GitRuntimeFiles> {
    let shim_dir = log_dir.join(format!("run-{run_id}-bin"));
    fs::create_dir_all(&shim_dir)
        .context_with(|| format!("failed to create git shim dir {}", shim_dir.display()))?;
    let policy_path = log_dir.join(format!("run-{run_id}.git-policy.json"));
    let runtime_policy = git_runtime_policy_for_run(settings, mutability);
    let policy_json = serde_json::to_string_pretty(&runtime_policy)
        .context("failed to encode git runtime policy")?;
    fs::write(&policy_path, policy_json)
        .context_with(|| format!("failed to write git policy {}", policy_path.display()))?;
    let shim_path = shim_dir.join("git");
    fs::write(
        &shim_path,
        format!(
            "#!/bin/sh\nexec {} git \"$@\"\n",
            shell_quote(&patchbay_binary.to_string_lossy())
        ),
    )
    .context_with(|| format!("failed to write git shim {}", shim_path.display()))?;
    mark_executable(&shim_path)?;
    Ok(GitRuntimeFiles {
        shim_dir,
        policy_path,
    })
}

pub(crate) fn git_runtime_policy_for_run(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> AgentGitRuntimePolicy {
    match mutability {
        AutomationRunMutability::Mutating => AgentGitRuntimePolicy {
            policy: settings.agent_git_command_policy.clone(),
            workspace_mode: settings.workspace_mode,
        },
        AutomationRunMutability::ReadOnly => AgentGitRuntimePolicy {
            policy: read_only_git_command_policy(),
            workspace_mode: WorkspaceMode::CurrentBranch,
        },
    }
}

pub(crate) fn resolve_real_git_path() -> Result<PathBuf> {
    let path = std::env::var_os("PATH").ok_or_else(|| report!("PATH is not set"))?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("git");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("git was not found on PATH")
}

pub(crate) fn agent_environment(
    patchbay_binary: &Path,
    git_runtime: &GitRuntimeFiles,
    real_git_path: &Path,
    project_name: &str,
    agent_id: &str,
    claimed_item_id: Option<i64>,
    api_url: Option<&str>,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let path = std::env::var("PATH").unwrap_or_default();
    if let Some(bin_dir) = patchbay_binary.parent() {
        env.insert(
            "PATH".to_owned(),
            format!(
                "{}:{}:{path}",
                git_runtime.shim_dir.to_string_lossy(),
                bin_dir.to_string_lossy()
            ),
        );
    } else {
        env.insert(
            "PATH".to_owned(),
            format!("{}:{path}", git_runtime.shim_dir.to_string_lossy()),
        );
    }
    env.insert(
        "PATCHBAY_GIT_POLICY_PATH".to_owned(),
        git_runtime.policy_path.to_string_lossy().into_owned(),
    );
    env.insert(
        "PATCHBAY_REAL_GIT".to_owned(),
        real_git_path.to_string_lossy().into_owned(),
    );
    env.insert("PATCHBAY_PROJECT".to_owned(), project_name.to_owned());
    env.insert("PATCHBAY_AGENT_ID".to_owned(), agent_id.to_owned());
    if let Some(item_id) = claimed_item_id {
        env.insert("PATCHBAY_CLAIMED_ITEM_ID".to_owned(), item_id.to_string());
    }
    if let Some(api_url) = api_url {
        env.insert("PATCHBAY_API_URL".to_owned(), api_url.to_owned());
    }
    env
}

pub(crate) fn agent_sandbox_mode_for_run(
    mutability: AutomationRunMutability,
    mode: AgentSandboxMode,
) -> SandboxMode {
    match mutability {
        AutomationRunMutability::Mutating => agent_sandbox_mode(mode),
        AutomationRunMutability::ReadOnly => SandboxMode::ReadOnly,
    }
}

pub(crate) fn agent_sandbox_policy_for_run(
    mutability: AutomationRunMutability,
    mode: AgentSandboxMode,
    agent_extra_writable_roots: &[String],
) -> serde_json::Value {
    match mutability {
        AutomationRunMutability::Mutating => agent_sandbox_policy(mode, agent_extra_writable_roots),
        AutomationRunMutability::ReadOnly => serde_json::json!({
            "type": "readOnly",
            "networkAccess": true,
        }),
    }
}

pub(crate) fn codex_memory_config_overrides() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::from_iter([
        (
            "features.memories".to_owned(),
            serde_json::Value::Bool(false),
        ),
        (
            "memories.use_memories".to_owned(),
            serde_json::Value::Bool(false),
        ),
        (
            "memories.generate_memories".to_owned(),
            serde_json::Value::Bool(false),
        ),
    ])
}

pub(crate) fn to_codex_reasoning(effort: AgentReasoningEffort) -> CodexReasoningEffort {
    match effort {
        AgentReasoningEffort::None => CodexReasoningEffort::None,
        AgentReasoningEffort::Minimal => CodexReasoningEffort::Minimal,
        AgentReasoningEffort::Low => CodexReasoningEffort::Low,
        AgentReasoningEffort::Medium => CodexReasoningEffort::Medium,
        AgentReasoningEffort::High => CodexReasoningEffort::High,
        AgentReasoningEffort::XHigh => CodexReasoningEffort::XHigh,
    }
}

fn read_only_git_command_policy() -> patchbay_types::AgentGitCommandPolicy {
    patchbay_types::AgentGitCommandPolicy {
        add: false,
        commit: false,
        push: false,
        reset: false,
        hard_reset: AgentGitHardResetPolicy::Never,
    }
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .context_with(|| format!("failed to stat {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    Ok(fs::set_permissions(path, permissions)
        .context_with(|| format!("failed to mark {} executable", path.display()))?)
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        return value.to_owned();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

fn agent_sandbox_mode(mode: AgentSandboxMode) -> SandboxMode {
    match mode {
        AgentSandboxMode::WorkspaceWrite => SandboxMode::WorkspaceWrite,
        AgentSandboxMode::DangerFullAccess => SandboxMode::DangerFullAccess,
    }
}

fn agent_sandbox_policy(
    mode: AgentSandboxMode,
    agent_extra_writable_roots: &[String],
) -> serde_json::Value {
    match mode {
        AgentSandboxMode::WorkspaceWrite => serde_json::json!({
            "type": "workspaceWrite",
            "networkAccess": true,
            "writableRoots": agent_extra_writable_roots,
        }),
        AgentSandboxMode::DangerFullAccess => serde_json::json!({
            "type": "dangerFullAccess",
        }),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::shared::view_models::{
        AgentGitCommandPolicy, AgentToolName, RevertStrategy, WorktreeCleanupPolicy,
    };

    fn settings_fixture() -> ProjectSettingsView {
        ProjectSettingsView {
            id: 1,
            project_id: 1,
            workspace_mode: WorkspaceMode::GitWorktree,
            max_code_edit_agents: 2,
            max_read_only_agents: 2,
            create_pr: false,
            auto_commit: true,
            commit_standard: String::new(),
            revert_strategy: RevertStrategy::Manual,
            stale_claim_minutes: 0,
            worktree_cleanup_policy: WorktreeCleanupPolicy::Manual,
            default_agent_tool: AgentToolName::Codex,
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite,
            agent_extra_writable_roots: Vec::new(),
            agent_git_command_policy: AgentGitCommandPolicy {
                add: true,
                commit: true,
                push: true,
                reset: true,
                hard_reset: AgentGitHardResetPolicy::IsolatedWorkspaces,
            },
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn agent_environment_exposes_api_but_not_database() {
        let git_runtime = GitRuntimeFiles {
            shim_dir: PathBuf::from("/tmp/patchbay-run-bin"),
            policy_path: PathBuf::from("/tmp/patchbay-git-policy.json"),
        };
        let env = agent_environment(
            Path::new("/tmp/patchbay"),
            &git_runtime,
            Path::new("/usr/bin/git"),
            "demo",
            "patchbay-run-1",
            Some(42),
            Some("http://127.0.0.1:4000"),
        );

        assert_eq!(
            env.get("PATCHBAY_PROJECT").map(String::as_str),
            Some("demo")
        );
        assert_eq!(
            env.get("PATCHBAY_AGENT_ID").map(String::as_str),
            Some("patchbay-run-1")
        );
        assert_eq!(
            env.get("PATCHBAY_CLAIMED_ITEM_ID").map(String::as_str),
            Some("42")
        );
        assert_eq!(
            env.get("PATCHBAY_API_URL").map(String::as_str),
            Some("http://127.0.0.1:4000")
        );
        assert_eq!(
            env.get("PATCHBAY_GIT_POLICY_PATH").map(String::as_str),
            Some("/tmp/patchbay-git-policy.json")
        );
        assert_eq!(
            env.get("PATCHBAY_REAL_GIT").map(String::as_str),
            Some("/usr/bin/git")
        );
        assert!(
            env.get("PATH")
                .is_some_and(|path| path.starts_with("/tmp/patchbay-run-bin:"))
        );
        assert!(!env.contains_key("PATCHBAY_DATABASE"));
        assert!(!env.contains_key("PATCHBAY_URL"));
    }

    #[test]
    fn codex_thread_config_disables_internal_memory() {
        let config = codex_memory_config_overrides();

        assert_eq!(
            config.get("features.memories"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            config.get("memories.use_memories"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            config.get("memories.generate_memories"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn codex_thread_sandbox_uses_project_writable_roots() {
        let roots = vec![
            "/tmp/patchbay-browser".to_owned(),
            "/Users/test/.patchbay/codex".to_owned(),
        ];
        let policy = agent_sandbox_policy(AgentSandboxMode::WorkspaceWrite, &roots);

        assert_eq!(
            policy,
            serde_json::json!({
                "type": "workspaceWrite",
                "networkAccess": true,
                "writableRoots": roots,
            })
        );
    }

    #[test]
    fn codex_thread_sandbox_can_disable_sandbox_for_project() {
        let roots = vec!["/tmp/ignored-when-full-access".to_owned()];

        assert_eq!(
            agent_sandbox_mode(AgentSandboxMode::DangerFullAccess),
            SandboxMode::DangerFullAccess
        );
        assert_eq!(
            agent_sandbox_policy(AgentSandboxMode::DangerFullAccess, &roots),
            serde_json::json!({
                "type": "dangerFullAccess",
            })
        );
    }

    #[test]
    fn read_only_codex_thread_sandbox_ignores_project_writable_roots() {
        let roots = vec!["/tmp/ignored-for-read-only".to_owned()];

        assert_eq!(
            agent_sandbox_mode_for_run(
                AutomationRunMutability::ReadOnly,
                AgentSandboxMode::DangerFullAccess
            ),
            SandboxMode::ReadOnly
        );
        assert_eq!(
            agent_sandbox_policy_for_run(
                AutomationRunMutability::ReadOnly,
                AgentSandboxMode::WorkspaceWrite,
                &roots
            ),
            serde_json::json!({
                "type": "readOnly",
                "networkAccess": true,
            })
        );
    }

    #[test]
    fn read_only_git_runtime_policy_disables_mutable_commands() {
        let settings = settings_fixture();

        let policy = git_runtime_policy_for_run(&settings, AutomationRunMutability::ReadOnly);

        assert_eq!(policy.workspace_mode, WorkspaceMode::CurrentBranch);
        assert!(!policy.policy.add);
        assert!(!policy.policy.commit);
        assert!(!policy.policy.push);
        assert!(!policy.policy.reset);
        assert_eq!(policy.policy.hard_reset, AgentGitHardResetPolicy::Never);
    }

    #[test]
    fn mutating_git_runtime_policy_uses_project_settings() {
        let settings = settings_fixture();

        let policy = git_runtime_policy_for_run(&settings, AutomationRunMutability::Mutating);

        assert_eq!(policy.workspace_mode, WorkspaceMode::GitWorktree);
        assert_eq!(policy.policy, settings.agent_git_command_policy);
    }

    #[test]
    fn prepare_git_runtime_writes_policy_and_shim() {
        let temp = TempDir::new().unwrap();
        let settings = settings_fixture();
        let patchbay_binary = temp.path().join("bin with spaces/patchbay");

        let runtime = prepare_git_runtime(
            7,
            temp.path(),
            &patchbay_binary,
            &settings,
            AutomationRunMutability::ReadOnly,
        )
        .unwrap();

        let policy = fs::read_to_string(&runtime.policy_path).unwrap();
        assert!(policy.contains("\"add\": false"));
        let shim = fs::read_to_string(runtime.shim_dir.join("git")).unwrap();
        assert!(shim.contains("exec '"));
        assert!(shim.contains("bin with spaces/patchbay' git"));
    }
}
