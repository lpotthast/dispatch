use super::{model::WorkspaceOpenTarget, runtime::WorkspaceRuntime};
use crate::backend::{
    projects::repository::ProjectRepository, runs::queries::service::RunQueryService,
};
use dispatch_types::WorkspaceEditorView;
use rootcause::{Result, prelude::*};
use std::{path::PathBuf, sync::Arc};
pub(crate) struct WorkspaceService {
    projects: Arc<ProjectRepository>,
    runs: Arc<RunQueryService>,
    runtime: Arc<WorkspaceRuntime>,
    database: PathBuf,
}
impl WorkspaceService {
    pub(crate) async fn choose_folder(&self) -> Result<Option<String>> {
        self.runtime.choose_folder().await
    }

    pub(crate) fn new(
        projects: Arc<ProjectRepository>,
        runs: Arc<RunQueryService>,
        runtime: Arc<WorkspaceRuntime>,
        database: PathBuf,
    ) -> Self {
        Self {
            projects,
            runs,
            runtime,
            database,
        }
    }
    pub(crate) async fn open_project(
        &self,
        project_name: &str,
        target: WorkspaceOpenTarget,
    ) -> Result<()> {
        let project = self.projects.by_name(project_name).await?;
        let path = project
            .path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| report!("project '{project_name}' has no workspace path"))?;
        self.runtime.open(target, path).await
    }
    pub(crate) async fn open_run(
        &self,
        project: &str,
        run_id: i64,
        target: WorkspaceOpenTarget,
    ) -> Result<()> {
        let run = self.runs.get(project, run_id).await?;
        self.runtime.open(target, run.working_dir).await
    }
    pub(crate) async fn open_database_directory(&self) -> Result<()> {
        let directory = self
            .database
            .parent()
            .ok_or_else(|| report!("database path has no parent directory"))?;
        self.runtime
            .open(WorkspaceOpenTarget::Folder, directory)
            .await
    }
    pub(crate) fn available_editors(&self) -> Vec<WorkspaceEditorView> {
        self.runtime.available_editors()
    }
}
