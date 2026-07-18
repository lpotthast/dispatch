use std::{env, process::Command as StdCommand};

use dispatch_types::AgentGitRuntimePolicy;
use rootcause::{Result, prelude::*};

pub(crate) fn run_git(args: Vec<String>) -> Result<()> {
    let policy_path = env::var("DISPATCH_GIT_POLICY_PATH")
        .context("DISPATCH_GIT_POLICY_PATH is required for dispatch git")?;
    let real_git =
        env::var("DISPATCH_REAL_GIT").context("DISPATCH_REAL_GIT is required for dispatch git")?;
    let policy_json = std::fs::read_to_string(&policy_path)
        .context_with(|| format!("failed to read git policy {policy_path}"))?;
    let runtime_policy: AgentGitRuntimePolicy =
        serde_json::from_str(&policy_json).context("failed to parse git policy")?;
    let checked_args = checked_git_args(args, &runtime_policy)?;
    let status = StdCommand::new(&real_git)
        .args(&checked_args)
        .status()
        .context_with(|| format!("failed to run real git at {real_git}"))?;
    if !status.success() {
        bail!("git exited with status {status}");
    }
    Ok(())
}

fn checked_git_args(
    args: Vec<String>,
    runtime_policy: &AgentGitRuntimePolicy,
) -> Result<Vec<String>> {
    let Some(command_index) = protected_git_command_index(&args) else {
        if args.is_empty() {
            bail!("git command is required");
        }
        return Ok(args);
    };
    let Some(command) = args.get(command_index).map(String::as_str) else {
        bail!("git command is required");
    };
    match command {
        "add" => {
            if !runtime_policy.policy.add {
                bail!("git add is not allowed by this Dispatch project policy");
            }
            Ok(args)
        }
        "commit" => checked_git_commit_args(args, runtime_policy, command_index),
        "push" => checked_git_push_args(args, runtime_policy, command_index),
        "reset" => checked_git_reset_args(args, runtime_policy, command_index),
        _ => Ok(args),
    }
}

fn protected_git_command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if matches!(arg, "add" | "commit" | "push" | "reset") {
            return Some(index);
        }
        if arg == "--" {
            return None;
        }
        if git_global_option_takes_separate_value(arg) {
            index += 2;
            continue;
        }
        if git_global_option_with_inline_value(arg) || arg.starts_with('-') {
            index += 1;
            continue;
        }
        return None;
    }
    None
}

fn git_global_option_takes_separate_value(arg: &str) -> bool {
    matches!(
        arg,
        "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" | "--config-env"
    )
}

fn git_global_option_with_inline_value(arg: &str) -> bool {
    [
        "--git-dir=",
        "--work-tree=",
        "--namespace=",
        "--exec-path=",
        "--config-env=",
    ]
    .iter()
    .any(|prefix| arg.starts_with(prefix))
}

fn checked_git_commit_args(
    mut args: Vec<String>,
    runtime_policy: &AgentGitRuntimePolicy,
    command_index: usize,
) -> Result<Vec<String>> {
    if !runtime_policy.policy.commit {
        bail!("git commit is not allowed by this Dispatch project policy");
    }
    if args
        .iter()
        .skip(command_index + 1)
        .any(|arg| arg == "--verify")
    {
        bail!("git commit --verify is blocked; Dispatch requires --no-verify");
    }
    if !args
        .iter()
        .skip(command_index + 1)
        .any(|arg| arg == "--no-verify")
    {
        args.insert(command_index + 1, "--no-verify".to_owned());
    }
    Ok(args)
}

fn checked_git_push_args(
    args: Vec<String>,
    runtime_policy: &AgentGitRuntimePolicy,
    command_index: usize,
) -> Result<Vec<String>> {
    if !runtime_policy.policy.push {
        bail!("git push is not allowed by this Dispatch project policy");
    }
    for arg in args.iter().skip(command_index + 1) {
        if arg == "-f"
            || is_short_force_push_flag(arg)
            || arg.starts_with("--force")
            || arg == "--mirror"
            || arg.starts_with("--mirror=")
            || arg == "--delete"
            || arg.starts_with("--delete=")
            || arg == "--prune"
            || arg.starts_with("--prune=")
            || arg.starts_with(':')
            || arg.starts_with('+')
        {
            bail!(
                "force, mirror, prune, delete, empty-source delete-refspec, and +ref pushes are blocked by Dispatch"
            );
        }
    }
    Ok(args)
}

fn is_short_force_push_flag(arg: &str) -> bool {
    arg.starts_with('-') && !arg.starts_with("--") && arg.chars().skip(1).any(|ch| ch == 'f')
}

fn checked_git_reset_args(
    args: Vec<String>,
    runtime_policy: &AgentGitRuntimePolicy,
    command_index: usize,
) -> Result<Vec<String>> {
    if !runtime_policy.policy.reset {
        bail!("git reset is not allowed by this Dispatch project policy");
    }
    for arg in args.iter().skip(command_index + 1) {
        if (arg == "--hard" || arg.starts_with("--hard="))
            && !runtime_policy
                .policy
                .allows_hard_reset(runtime_policy.workspace_mode)
        {
            bail!("git reset --hard is blocked for this Dispatch workspace mode");
        }
        if arg == "--merge"
            || arg.starts_with("--merge=")
            || arg == "--keep"
            || arg.starts_with("--keep=")
            || arg == "--recurse-submodules"
            || arg.starts_with("--recurse-submodules=")
        {
            bail!("git reset mode '{arg}' is blocked by Dispatch");
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use dispatch_types::{AgentGitCommandPolicy, WorkspaceMode};

    use super::*;

    fn git_runtime_policy(
        policy: AgentGitCommandPolicy,
        workspace_mode: WorkspaceMode,
    ) -> AgentGitRuntimePolicy {
        AgentGitRuntimePolicy {
            policy,
            workspace_mode,
        }
    }

    #[test]
    fn git_commit_policy_injects_no_verify_and_rejects_verify() {
        let policy = git_runtime_policy(Default::default(), WorkspaceMode::CurrentBranch);

        let args = checked_git_args(
            vec![
                "commit".to_owned(),
                "-m".to_owned(),
                "Update docs".to_owned(),
            ],
            &policy,
        )
        .unwrap();
        assert_that!(&(args[0])).is_equal_to("commit");
        assert_that!(&(args[1])).is_equal_to("--no-verify");

        let err = checked_git_args(vec!["commit".to_owned(), "--verify".to_owned()], &policy)
            .unwrap_err();
        assert_that!(&(err.to_string().contains("--verify is blocked"))).is_true();
    }

    #[test]
    fn git_policy_detects_protected_commands_after_global_options() {
        let policy = git_runtime_policy(Default::default(), WorkspaceMode::GitWorktree);

        let args = checked_git_args(
            vec![
                "-C".to_owned(),
                "/tmp/repo".to_owned(),
                "-c".to_owned(),
                "user.name=Dispatch".to_owned(),
                "commit".to_owned(),
                "-m".to_owned(),
                "Update docs".to_owned(),
            ],
            &policy,
        )
        .unwrap();
        assert_that!(&(args[4])).is_equal_to("commit");
        assert_that!(&(args[5])).is_equal_to("--no-verify");

        let err = checked_git_args(
            vec![
                "--git-dir=/tmp/repo/.git".to_owned(),
                "--work-tree".to_owned(),
                "/tmp/repo".to_owned(),
                "push".to_owned(),
                "--force".to_owned(),
            ],
            &policy,
        )
        .unwrap_err();
        assert_that!(&(err.to_string().contains("blocked by Dispatch"))).is_true();
    }

    #[test]
    fn git_push_policy_rejects_force_delete_and_plus_ref_pushes() {
        let policy = git_runtime_policy(Default::default(), WorkspaceMode::GitWorktree);

        for args in [
            vec!["push", "-f"],
            vec!["push", "--force-with-lease"],
            vec!["push", "--mirror"],
            vec!["push", "--delete"],
            vec!["push", "--prune"],
            vec!["push", "origin", "+HEAD:main"],
            vec!["push", "origin", ":obsolete"],
        ] {
            let err = checked_git_args(args.into_iter().map(str::to_owned).collect(), &policy)
                .unwrap_err();
            assert_that!(&(err.to_string().contains("blocked by Dispatch"))).is_true();
        }

        checked_git_args(
            vec!["push".to_owned(), "origin".to_owned(), "HEAD".to_owned()],
            &policy,
        )
        .unwrap();
    }

    #[test]
    fn git_reset_policy_allows_hard_only_for_isolated_workspaces() {
        let current_branch = git_runtime_policy(Default::default(), WorkspaceMode::CurrentBranch);
        let err = checked_git_args(
            vec!["reset".to_owned(), "--hard".to_owned(), "HEAD".to_owned()],
            &current_branch,
        )
        .unwrap_err();
        assert_that!(
            &(err
                .to_string()
                .contains("blocked for this Dispatch workspace mode"))
        )
        .is_true();

        let worktree = git_runtime_policy(Default::default(), WorkspaceMode::GitWorktree);
        checked_git_args(
            vec!["reset".to_owned(), "--hard".to_owned(), "HEAD".to_owned()],
            &worktree,
        )
        .unwrap();

        let err = checked_git_args(vec!["reset".to_owned(), "--merge".to_owned()], &worktree)
            .unwrap_err();
        assert_that!(&(err.to_string().contains("blocked by Dispatch"))).is_true();
    }

    #[test]
    fn disabled_git_commands_are_rejected() {
        let policy = git_runtime_policy(
            AgentGitCommandPolicy {
                add: false,
                commit: false,
                push: false,
                reset: false,
                ..Default::default()
            },
            WorkspaceMode::GitBranch,
        );

        for command in ["add", "commit", "push", "reset"] {
            let err = checked_git_args(vec![command.to_owned()], &policy).unwrap_err();
            assert_that!(&(err.to_string().contains("not allowed"))).is_true();
        }
    }
}
