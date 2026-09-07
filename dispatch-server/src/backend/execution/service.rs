use super::{
    model::{AgentProcessOutput, AgentProcessStart},
    sessions::ProcessSessionRegistry,
};
use rootcause::Result;
use std::sync::Arc;
use tokio::sync::watch;
/// Shared process execution receives prepared inputs and returns an outcome to its owning workflow.
/// It never claims work items, changes knowledge jobs, or opens the operational database.
pub(crate) struct AgentExecutionService {
    server_api_url: Arc<str>,
    git: Arc<super::git::GitRuntime>,
    runtime: Arc<super::runtime::AgentRuntime>,
    sessions: Option<ProcessSessionRegistry>,
}

impl AgentExecutionService {
    pub(crate) fn new(
        server_api_url: impl Into<Arc<str>>,
        git: Arc<super::git::GitRuntime>,
        runtime: Arc<super::runtime::AgentRuntime>,
        sessions: Option<ProcessSessionRegistry>,
    ) -> Self {
        Self {
            server_api_url: server_api_url.into(),
            git,
            runtime,
            sessions,
        }
    }
    pub(crate) fn api_url(&self) -> &str {
        &self.server_api_url
    }

    pub(crate) async fn execute(
        &self,
        mut start: AgentProcessStart,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> Result<AgentProcessOutput> {
        if start.environment.is_none() {
            start.environment = Some(self.git.agent_environment(
                &start.dispatch_binary,
                &start.git_runtime,
                &start.real_git_path,
                &start.project_name,
                &start.agent_id,
                start.claimed_item_id,
                Some(self.api_url()),
            ));
        }
        self.runtime
            .execute(start, self.sessions.clone(), cancellation)
            .await
    }
}
