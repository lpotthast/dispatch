use rootcause::{Result, prelude::*};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};
pub(crate) struct LaunchRuntime {
    log_directory: PathBuf,
    pub(super) workspaces:
        std::sync::Arc<crate::backend::execution::workspaces::runs::RunWorkspaceRuntime>,
    pub(super) commits: std::sync::Arc<super::commit::CommitRuntime>,
}
impl LaunchRuntime {
    pub(crate) fn new(
        log_directory: PathBuf,
        workspaces: std::sync::Arc<
            crate::backend::execution::workspaces::runs::RunWorkspaceRuntime,
        >,
        commits: std::sync::Arc<super::commit::CommitRuntime>,
    ) -> Self {
        Self {
            log_directory,
            workspaces,
            commits,
        }
    }
    pub(crate) fn log_directory(&self) -> PathBuf {
        self.log_directory.clone()
    }
    pub(crate) fn prepare_directory(&self, path: &Path) -> std::io::Result<()> {
        fs::create_dir_all(path)
    }
    pub(crate) fn write(&self, path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        fs::write(path, contents)
    }
    pub(crate) async fn create_pull_request(&self, working_dir: &Path) -> Result<String> {
        let working_dir = working_dir.to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            StdCommand::new("gh")
                .arg("pr")
                .arg("create")
                .arg("--fill")
                .current_dir(working_dir)
                .output()
        })
        .await
        .context("PR creation task failed")?
        .context("failed to start gh pr create")?;
        if !output.status.success() {
            bail!(
                "gh pr create failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}
