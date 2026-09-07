use super::model::RunArtifactLocations;
use rootcause::{Result, prelude::*};
use std::path::{Path, PathBuf};
pub(crate) struct RunCleanupRuntime {
    run_artifact_dir: PathBuf,
    workspaces: std::sync::Arc<crate::backend::execution::workspaces::runs::RunWorkspaceRuntime>,
}
impl RunCleanupRuntime {
    pub(crate) fn new(
        run_artifact_dir: PathBuf,
        workspaces: std::sync::Arc<
            crate::backend::execution::workspaces::runs::RunWorkspaceRuntime,
        >,
    ) -> Self {
        Self {
            run_artifact_dir,
            workspaces,
        }
    }
    pub(crate) fn cleanup(
        &self,
        project: &crate::backend::projects::model::ProjectScope,
        runs: &[RunArtifactLocations],
    ) -> Result<()> {
        self.cleanup_run_workspaces(project, runs)?;
        cleanup_run_artifacts(&self.run_artifact_dir, runs.iter().map(|run| run.id))
    }
    fn cleanup_run_workspaces(
        &self,
        project: &crate::backend::projects::model::ProjectScope,
        runs: &[crate::backend::runs::model::RunArtifactLocations],
    ) -> Result<()> {
        for run in runs {
            if run.branch_name.is_none() && run.worktree_path.is_none() {
                continue;
            }
            let repo_path = project.path.as_deref().ok_or_else(|| {
                report!(
                    "cannot clean workspace artifacts for run {} because project '{}' has no workspace path",
                    run.id,
                    project.name
                )
            })?;
            self.workspaces
                .remove_run_workspace(
                    Path::new(repo_path),
                    run.branch_name.as_deref(),
                    run.worktree_path.as_deref().map(Path::new),
                )
                .context_with(|| {
                    format!("failed to clean workspace artifacts for run {}", run.id)
                })?;
        }
        Ok(())
    }
}
fn cleanup_run_artifacts(
    run_artifact_dir: &Path,
    run_ids: impl IntoIterator<Item = i64>,
) -> Result<()> {
    for run_id in run_ids {
        for suffix in [
            "developer-instructions.md",
            "user-prompt.md",
            "output.json",
            "incremental.jsonl",
            "codex-stderr.log",
            "git-policy.json",
        ] {
            remove_path_if_exists(&run_artifact_dir.join(format!("run-{run_id}.{suffix}")))?;
        }
        remove_path_if_exists(&run_artifact_dir.join(format!("run-{run_id}-bin")))?;
    }
    Ok(())
}

pub(crate) fn remove_path_if_exists(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            Err(err).context_with(|| format!("failed to inspect '{}'", path.display()))?;
            unreachable!("filesystem inspection error must propagate");
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
            .context_with(|| format!("failed to remove directory '{}'", path.display()))?;
    } else {
        std::fs::remove_file(path)
            .context_with(|| format!("failed to remove file '{}'", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use std::fs;
    use tempfile::TempDir;
    #[test]
    fn run_artifact_cleanup_removes_every_dispatch_file_for_run() {
        let temp = TempDir::new().unwrap();
        for suffix in [
            "developer-instructions.md",
            "user-prompt.md",
            "output.json",
            "incremental.jsonl",
            "codex-stderr.log",
            "git-policy.json",
        ] {
            fs::write(temp.path().join(format!("run-42.{suffix}")), suffix).unwrap();
        }
        let shim_dir = temp.path().join("run-42-bin");
        fs::create_dir(&shim_dir).unwrap();
        fs::write(shim_dir.join("git"), "shim").unwrap();
        fs::write(temp.path().join("run-7.output.json"), "keep").unwrap();

        cleanup_run_artifacts(temp.path(), [42]).unwrap();

        assert_that!(&(fs::read_dir(temp.path()).unwrap().count())).is_equal_to(1);
        assert_that!(&(temp.path().join("run-7.output.json").exists())).is_true();
    }
}
