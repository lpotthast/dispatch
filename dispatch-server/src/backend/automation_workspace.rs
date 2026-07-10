use std::{
    fs,
    path::{Path, PathBuf},
};

use git2::{
    Repository, StatusOptions, WorktreeAddOptions, WorktreePruneOptions, build::CheckoutBuilder,
};
use rootcause::{Result, prelude::*};

use crate::shared::view_models::{AutomationRunMutability, WorkspaceMode};

pub(crate) struct WorkspacePlan {
    pub(crate) working_dir: PathBuf,
    pub(crate) worktree_path: Option<PathBuf>,
    pub(crate) branch_name: Option<String>,
}

pub(crate) fn prepare_workspace_for_run(
    run_id: i64,
    project_name: &str,
    project_path: &Path,
    workspace_mode: WorkspaceMode,
    mutability: AutomationRunMutability,
) -> Result<WorkspacePlan> {
    if mutability == AutomationRunMutability::ReadOnly {
        return prepare_read_only_workspace(project_path);
    }
    prepare_workspace(run_id, project_name, project_path, workspace_mode)
}

pub(crate) fn prune_git_worktree(
    repo_path: &Path,
    branch_name: &str,
    worktree_path: &Path,
) -> Result<()> {
    let repo = Repository::open(repo_path)
        .context_with(|| format!("failed to open git repository '{}'", repo_path.display()))?;
    match repo.find_worktree(&worktree_name(branch_name)) {
        Ok(worktree) => {
            let mut prune_options = WorktreePruneOptions::new();
            prune_options.valid(true).working_tree(true);
            worktree.prune(Some(&mut prune_options)).context_with(|| {
                format!("failed to prune git worktree '{}'", worktree_path.display())
            })?;
        }
        Err(err) => {
            if !worktree_path.exists() {
                return Ok(());
            }
            fs::remove_dir_all(worktree_path).context_with(|| {
                format!(
                    "failed to remove stale worktree directory '{}' after git lookup failed: {err}",
                    worktree_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn prepare_read_only_workspace(project_path: &Path) -> Result<WorkspacePlan> {
    if !project_path.is_dir() {
        bail!("path '{}' is not a directory", project_path.display());
    }
    Ok(WorkspacePlan {
        working_dir: project_path.to_path_buf(),
        worktree_path: None,
        branch_name: None,
    })
}

fn prepare_workspace(
    run_id: i64,
    project_name: &str,
    project_path: &Path,
    workspace_mode: WorkspaceMode,
) -> Result<WorkspacePlan> {
    if !project_path.is_dir() {
        bail!("path '{}' is not a directory", project_path.display());
    }

    match workspace_mode {
        WorkspaceMode::CurrentBranch => Ok(WorkspacePlan {
            working_dir: project_path.to_path_buf(),
            worktree_path: None,
            branch_name: None,
        }),
        WorkspaceMode::GitWorktree => {
            let slug = slugify(project_name);
            let root = project_path
                .parent()
                .unwrap_or(project_path)
                .join(".dispatch-worktrees");
            let worktree_path = root.join(format!("{slug}-{run_id}"));
            let branch_name = format!("dispatch/{slug}-{run_id}");
            fs::create_dir_all(&root)
                .context_with(|| format!("failed to create {}", root.display()))?;
            create_git_worktree(project_path, &branch_name, &worktree_path)?;
            Ok(WorkspacePlan {
                working_dir: worktree_path.clone(),
                worktree_path: Some(worktree_path),
                branch_name: Some(branch_name),
            })
        }
        WorkspaceMode::GitBranch => {
            let branch_name = format!("dispatch/{}-{}", slugify(project_name), run_id);
            create_and_checkout_git_branch(project_path, &branch_name)?;
            Ok(WorkspacePlan {
                working_dir: project_path.to_path_buf(),
                worktree_path: None,
                branch_name: Some(branch_name),
            })
        }
    }
}

fn ensure_git_worktree_clean(path: &Path) -> Result<()> {
    if !git_worktree_is_clean(path)? {
        bail!(
            "current workspace '{}' has uncommitted changes",
            path.display()
        );
    }
    Ok(())
}

fn git_worktree_is_clean(path: &Path) -> Result<bool> {
    let repo = Repository::open(path)
        .context_with(|| format!("failed to open git repository '{}'", path.display()))?;
    let mut status_options = StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut status_options))
        .context_with(|| format!("failed to read git status for '{}'", path.display()))?;
    Ok(statuses.is_empty())
}

fn create_and_checkout_git_branch(repo_path: &Path, branch_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path)
        .context_with(|| format!("failed to open git repository '{}'", repo_path.display()))?;
    ensure_git_worktree_clean(repo_path)?;
    let head = repo.head().context("failed to read repository HEAD")?;
    let target = head
        .peel_to_commit()
        .context("repository HEAD does not point to a commit")?;
    repo.branch(branch_name, &target, false)
        .context_with(|| format!("failed to create branch '{branch_name}'"))?;
    repo.set_head(&format!("refs/heads/{branch_name}"))
        .context_with(|| format!("failed to set HEAD to '{branch_name}'"))?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_head(Some(&mut checkout))
        .context_with(|| format!("failed to check out branch '{branch_name}'"))?;
    Ok(())
}

fn create_git_worktree(repo_path: &Path, branch_name: &str, worktree_path: &Path) -> Result<()> {
    let repo = Repository::open(repo_path)
        .context_with(|| format!("failed to open git repository '{}'", repo_path.display()))?;
    let head = repo.head().context("failed to read repository HEAD")?;
    let target = head
        .peel_to_commit()
        .context("repository HEAD does not point to a commit")?;
    repo.branch(branch_name, &target, false)
        .context_with(|| format!("failed to create branch '{branch_name}'"))?;
    let branch_reference = repo
        .find_reference(&format!("refs/heads/{branch_name}"))
        .context_with(|| format!("failed to read branch reference '{branch_name}'"))?;
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(&branch_reference));
    repo.worktree(
        worktree_name(branch_name).as_str(),
        worktree_path,
        Some(&options),
    )
    .context_with(|| format!("failed to create worktree '{}'", worktree_path.display()))?;
    Ok(())
}

fn worktree_name(branch_name: &str) -> String {
    branch_name.replace('/', "-")
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "project".to_owned()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn current_branch_accepts_non_git_directory() {
        let temp = TempDir::new().unwrap();

        let plan = prepare_workspace(1, "demo", temp.path(), WorkspaceMode::CurrentBranch).unwrap();

        assert_eq!(plan.working_dir, temp.path());
        assert!(plan.worktree_path.is_none());
        assert!(plan.branch_name.is_none());
    }

    #[test]
    fn current_branch_accepts_dirty_unborn_git_repository() {
        let temp = TempDir::new().unwrap();
        Repository::init(temp.path()).unwrap();
        fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let plan = prepare_workspace(1, "demo", temp.path(), WorkspaceMode::CurrentBranch).unwrap();

        assert_eq!(plan.working_dir, temp.path());
        assert!(plan.worktree_path.is_none());
        assert!(plan.branch_name.is_none());
    }

    #[test]
    fn read_only_workspace_uses_project_checkout_without_branch_or_worktree() {
        let temp = TempDir::new().unwrap();

        let plan = prepare_workspace_for_run(
            1,
            "demo",
            temp.path(),
            WorkspaceMode::GitWorktree,
            AutomationRunMutability::ReadOnly,
        )
        .unwrap();

        assert_eq!(plan.working_dir, temp.path());
        assert!(plan.worktree_path.is_none());
        assert!(plan.branch_name.is_none());
    }
}
