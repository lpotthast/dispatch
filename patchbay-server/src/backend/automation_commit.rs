use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

use git2::{ErrorCode as GitErrorCode, Oid, Repository, Sort, StatusOptions};
use rootcause::{Result, prelude::*};

use crate::shared::view_models::{AgentCommitOutcome, AutomationRunMutability};

#[derive(Clone, Debug)]
pub(crate) struct CommitBaseline {
    required: bool,
    inspection: std::result::Result<GitInspection, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CommitOutcomeEvaluation {
    pub(crate) outcome: AgentCommitOutcome,
    pub(crate) shas: Vec<String>,
    pub(crate) detail: Option<String>,
    pub(crate) validation_failed: bool,
}

#[derive(Clone, Debug)]
struct GitSnapshot {
    head: Option<Oid>,
    status: Vec<String>,
}

#[derive(Clone, Debug)]
enum GitInspection {
    Repository(GitSnapshot),
    NoRepository,
}

pub(crate) fn capture_commit_baseline(path: &Path, required: bool) -> CommitBaseline {
    CommitBaseline {
        required,
        inspection: inspect_git_workspace(path).map_err(|err| format!("{err:#}")),
    }
}

pub(crate) fn evaluate_commit_outcome_for_run(
    path: &Path,
    baseline: &CommitBaseline,
    mutability: AutomationRunMutability,
) -> CommitOutcomeEvaluation {
    if mutability == AutomationRunMutability::ReadOnly {
        return CommitOutcomeEvaluation {
            outcome: AgentCommitOutcome::NotRequired,
            shas: Vec::new(),
            detail: Some("commit is not required for read-only automation".to_owned()),
            validation_failed: false,
        };
    }
    evaluate_commit_outcome(path, baseline)
}

fn evaluate_commit_outcome(path: &Path, baseline: &CommitBaseline) -> CommitOutcomeEvaluation {
    let initial = match &baseline.inspection {
        Ok(inspection) => inspection,
        Err(err) => {
            return CommitOutcomeEvaluation {
                outcome: AgentCommitOutcome::Unknown,
                shas: Vec::new(),
                detail: Some(format!("failed to inspect git before launch: {err}")),
                validation_failed: false,
            };
        }
    };
    let final_inspection = match inspect_git_workspace(path) {
        Ok(inspection) => inspection,
        Err(err) => {
            return CommitOutcomeEvaluation {
                outcome: AgentCommitOutcome::Unknown,
                shas: Vec::new(),
                detail: Some(format!("failed to inspect git after run: {err:#}")),
                validation_failed: false,
            };
        }
    };

    let (initial_snapshot, final_snapshot) = match (initial, &final_inspection) {
        (GitInspection::NoRepository, GitInspection::NoRepository) => {
            return CommitOutcomeEvaluation {
                outcome: AgentCommitOutcome::SkippedNoGitRepo,
                shas: Vec::new(),
                detail: Some("workspace is not a git repository".to_owned()),
                validation_failed: false,
            };
        }
        (GitInspection::Repository(_), GitInspection::NoRepository) => {
            return CommitOutcomeEvaluation {
                outcome: AgentCommitOutcome::SkippedNoGitRepo,
                shas: Vec::new(),
                detail: Some("workspace git repository is no longer available".to_owned()),
                validation_failed: false,
            };
        }
        (GitInspection::NoRepository, GitInspection::Repository(final_snapshot)) => {
            (None, final_snapshot)
        }
        (
            GitInspection::Repository(initial_snapshot),
            GitInspection::Repository(final_snapshot),
        ) => (Some(initial_snapshot), final_snapshot),
    };

    let initial_head = initial_snapshot.and_then(|snapshot| snapshot.head);
    let commit_shas = match commit_shas_after(path, initial_head, final_snapshot.head) {
        Ok(commit_shas) => commit_shas,
        Err(err) => {
            return CommitOutcomeEvaluation {
                outcome: AgentCommitOutcome::Unknown,
                shas: Vec::new(),
                detail: Some(format!("failed to list commits created by run: {err:#}")),
                validation_failed: false,
            };
        }
    };
    if !commit_shas.is_empty() {
        return CommitOutcomeEvaluation {
            outcome: AgentCommitOutcome::Committed,
            shas: commit_shas,
            detail: None,
            validation_failed: false,
        };
    }

    let initial_status = initial_snapshot
        .map(|snapshot| snapshot.status.as_slice())
        .unwrap_or(&[]);
    let status_changed = initial_status != final_snapshot.status.as_slice();
    if !status_changed {
        return CommitOutcomeEvaluation {
            outcome: AgentCommitOutcome::SkippedNoChanges,
            shas: Vec::new(),
            detail: Some("no new commits or workspace changes were detected".to_owned()),
            validation_failed: false,
        };
    }

    if baseline.required {
        CommitOutcomeEvaluation {
            outcome: AgentCommitOutcome::MissingRequired,
            shas: Vec::new(),
            detail: Some(
                "workspace has uncommitted changes and no new commit was created".to_owned(),
            ),
            validation_failed: true,
        }
    } else {
        CommitOutcomeEvaluation {
            outcome: AgentCommitOutcome::NotRequired,
            shas: Vec::new(),
            detail: Some("commit was not required by the project policy".to_owned()),
            validation_failed: false,
        }
    }
}

fn inspect_git_workspace(path: &Path) -> Result<GitInspection> {
    let repo = match Repository::discover(path) {
        Ok(repo) => repo,
        Err(err) if err.code() == GitErrorCode::NotFound => {
            return Ok(GitInspection::NoRepository);
        }
        Err(err) => {
            bail!(
                "failed to open git repository for '{}': {err}",
                path.display()
            );
        }
    };
    Ok(GitInspection::Repository(git_snapshot(&repo)?))
}

fn git_snapshot(repo: &Repository) -> Result<GitSnapshot> {
    let head = match repo.head() {
        Ok(head) => Some(
            head.peel_to_commit()
                .context("repository HEAD does not point to a commit")?
                .id(),
        ),
        Err(err)
            if matches!(
                err.code(),
                GitErrorCode::UnbornBranch | GitErrorCode::NotFound
            ) =>
        {
            None
        }
        Err(err) => {
            bail!("failed to read repository HEAD: {err}");
        }
    };
    Ok(GitSnapshot {
        head,
        status: git_status_fingerprint(repo)?,
    })
}

fn git_status_fingerprint(repo: &Repository) -> Result<Vec<String>> {
    let mut status_options = StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut status_options))
        .context("failed to read git status")?;
    let index = repo.index().context("failed to read git index")?;
    let workdir = repo.workdir().map(Path::to_path_buf);
    let mut entries = statuses
        .iter()
        .map(|entry| {
            let path = entry.path().unwrap_or("(unknown)");
            let index_oid = index
                .get_path(Path::new(path), 0)
                .map(|entry| entry.id.to_string())
                .unwrap_or_else(|| "-".to_owned());
            let worktree_hash = workdir
                .as_deref()
                .and_then(|workdir| worktree_path_fingerprint(workdir, path))
                .unwrap_or_else(|| "-".to_owned());
            format!(
                "{:?}:{}:index={}:worktree={}",
                entry.status(),
                path,
                index_oid,
                worktree_hash
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn worktree_path_fingerprint(workdir: &Path, relative_path: &str) -> Option<String> {
    let path = workdir.join(relative_path);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink() {
        return fs::read_link(&path)
            .ok()
            .map(|target| format!("symlink:{}", target.display()));
    }
    if metadata.is_file() {
        let bytes = fs::read(&path).ok()?;
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        return Some(format!("file:{:016x}", hasher.finish()));
    }
    if metadata.is_dir() {
        return Some("dir".to_owned());
    }
    Some("special".to_owned())
}

fn commit_shas_after(
    path: &Path,
    initial_head: Option<Oid>,
    final_head: Option<Oid>,
) -> Result<Vec<String>> {
    let Some(final_head) = final_head else {
        return Ok(Vec::new());
    };
    if Some(final_head) == initial_head {
        return Ok(Vec::new());
    }

    let repo = Repository::discover(path)
        .context_with(|| format!("failed to open git repository for '{}'", path.display()))?;
    let mut revwalk = repo.revwalk().context("failed to create git revwalk")?;
    revwalk
        .push(final_head)
        .context("failed to add final HEAD to git revwalk")?;
    if let Some(initial_head) = initial_head {
        revwalk
            .hide(initial_head)
            .context("failed to hide baseline HEAD in git revwalk")?;
    }
    revwalk
        .set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)
        .context("failed to configure git revwalk sorting")?;
    revwalk
        .map(|oid| -> Result<String> {
            Ok(oid
                .context("failed to read commit id from git revwalk")?
                .to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn commit_all(repo: &Repository, message: &str) -> String {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Patchbay Test", "patchbay@example.com").unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents = parent.iter().collect::<Vec<_>>();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .unwrap()
        .to_string()
    }

    #[test]
    fn commit_outcome_skips_when_workspace_is_not_git_repo() {
        let temp = TempDir::new().unwrap();
        let baseline = capture_commit_baseline(temp.path(), true);

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::SkippedNoGitRepo);
        assert!(evaluation.shas.is_empty());
        assert!(!evaluation.validation_failed);
    }

    #[test]
    fn commit_outcome_skips_when_no_commit_or_workspace_change_exists() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "initial\n").unwrap();
        commit_all(&repo, "Initial commit");
        let baseline = capture_commit_baseline(temp.path(), true);

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::SkippedNoChanges);
        assert!(evaluation.shas.is_empty());
        assert!(!evaluation.validation_failed);
    }

    #[test]
    fn commit_outcome_records_created_commits() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "initial\n").unwrap();
        commit_all(&repo, "Initial commit");
        let baseline = capture_commit_baseline(temp.path(), true);
        fs::write(temp.path().join("README.md"), "initial\nchanged\n").unwrap();
        let created_sha = commit_all(&repo, "Update README");

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::Committed);
        assert_eq!(evaluation.shas, vec![created_sha]);
        assert!(!evaluation.validation_failed);
    }

    #[test]
    fn commit_outcome_fails_validation_when_required_commit_is_missing() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "initial\n").unwrap();
        commit_all(&repo, "Initial commit");
        let baseline = capture_commit_baseline(temp.path(), true);
        fs::write(temp.path().join("README.md"), "initial\nchanged\n").unwrap();

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::MissingRequired);
        assert!(evaluation.shas.is_empty());
        assert!(evaluation.validation_failed);
    }

    #[test]
    fn commit_outcome_detects_changes_to_preexisting_dirty_file() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "initial\n").unwrap();
        commit_all(&repo, "Initial commit");
        fs::write(temp.path().join("README.md"), "dirty before launch\n").unwrap();
        let baseline = capture_commit_baseline(temp.path(), true);
        fs::write(temp.path().join("README.md"), "dirty after launch\n").unwrap();

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::MissingRequired);
        assert!(evaluation.shas.is_empty());
        assert!(evaluation.validation_failed);
    }

    #[test]
    fn commit_outcome_allows_uncommitted_changes_when_commit_is_not_required() {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("README.md"), "initial\n").unwrap();
        commit_all(&repo, "Initial commit");
        let baseline = capture_commit_baseline(temp.path(), false);
        fs::write(temp.path().join("README.md"), "initial\nchanged\n").unwrap();

        let evaluation = evaluate_commit_outcome(temp.path(), &baseline);

        assert_eq!(evaluation.outcome, AgentCommitOutcome::NotRequired);
        assert!(evaluation.shas.is_empty());
        assert!(!evaluation.validation_failed);
    }

    #[test]
    fn read_only_commit_outcome_is_not_required_without_git_validation() {
        let temp = TempDir::new().unwrap();
        let baseline = capture_commit_baseline(temp.path(), false);

        let evaluation = evaluate_commit_outcome_for_run(
            temp.path(),
            &baseline,
            AutomationRunMutability::ReadOnly,
        );

        assert_eq!(evaluation.outcome, AgentCommitOutcome::NotRequired);
        assert!(evaluation.shas.is_empty());
        assert!(!evaluation.validation_failed);
    }
}
