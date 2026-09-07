use dispatch_types::ProjectGitStatusView;
use git2::{DiffOptions, ErrorCode as GitErrorCode, Oid, Repository};
use rootcause::{Result, prelude::*};
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct ProjectRuntime;
impl ProjectRuntime {
    pub(super) fn normalize_path(&self, path: PathBuf) -> Result<String> {
        normalize_project_path(path)
    }
    pub(super) fn prepare_path_update(
        &self,
        path: super::model::ProjectPathUpdate,
    ) -> Result<Option<String>> {
        match path {
            super::model::ProjectPathUpdate::Set(path) => self.normalize_path(path).map(Some),
            super::model::ProjectPathUpdate::Clear => Ok(None),
        }
    }
    pub(super) fn path_exists(&self, path: Option<&str>) -> bool {
        project_path_exists(path)
    }
    pub(super) fn git_status(
        &self,
        path: Option<&str>,
        exists: bool,
    ) -> Option<ProjectGitStatusView> {
        inspect_project_git_status(path, exists)
    }
    pub(super) fn validate_knowledge_directory(
        &self,
        project: &dispatch_types::ProjectView,
        value: &str,
    ) -> Result<String> {
        validate_knowledge_directory_update(project, value)
    }
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

pub(super) fn project_path_exists(path: Option<&str>) -> bool {
    path.map(expand_home_path)
        .is_some_and(|path| Path::new(&path).is_dir())
}

pub(super) fn inspect_project_git_status(
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

pub(crate) fn expand_home_path(path: &str) -> String {
    expand_home_path_with(path.trim(), env::var_os("HOME").as_ref())
}

pub(super) fn expand_home_path_with(path: &str, home: Option<&std::ffi::OsString>) -> String {
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

pub(crate) fn validate_knowledge_directory_update(
    project: &dispatch_types::ProjectView,
    value: &str,
) -> Result<String> {
    let knowledge_directory = crate::backend::knowledge::normalize_knowledge_directory(value)
        .context("project has invalid knowledge directory")?;
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

/// Filesystem and workspace cleanup requested after project runtime admission is closed.
pub(crate) struct ProjectDeletionRuntime {
    pub(crate) runs: std::sync::Arc<crate::backend::runs::cleanup::RunCleanupRuntime>,
    pub(crate) codex_projects_dir: PathBuf,
    pub(crate) knowledge_projects_dir: PathBuf,
}

impl ProjectDeletionRuntime {
    pub(crate) fn new(
        runs: std::sync::Arc<crate::backend::runs::cleanup::RunCleanupRuntime>,
        codex_projects_dir: PathBuf,
        knowledge_projects_dir: PathBuf,
    ) -> Self {
        Self {
            runs,
            codex_projects_dir,
            knowledge_projects_dir,
        }
    }
    pub(super) fn cleanup(
        &self,
        project: &super::model::ProjectScope,
        runs: &[crate::backend::runs::model::RunArtifactLocations],
        project_artifacts: &Path,
    ) -> Result<()> {
        self.runs.cleanup(project, runs)?;
        crate::backend::runs::cleanup::remove_path_if_exists(
            &self.codex_projects_dir.join(project.id.to_string()),
        )?;
        crate::backend::runs::cleanup::remove_path_if_exists(
            &self.knowledge_projects_dir.join(project.id.to_string()),
        )?;
        crate::backend::runs::cleanup::remove_path_if_exists(project_artifacts)?;
        Ok(())
    }
}
