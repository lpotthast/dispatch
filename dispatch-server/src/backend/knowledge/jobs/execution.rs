use super::execution_files::PassFiles;
use crate::backend::{
    automation::launch::prompt::{AutomationPrompt, effective_input_sha256},
    execution::identity as agent_ids,
    execution::output::write_run_output_log,
    execution::sessions::ProcessSessionRegistry,
    execution::workspaces::runs::WorkspacePlan,
    execution::{
        AgentExecutionService, AgentProcessStart, codex::service::CodexService,
        is_automation_cancelled, tools::service::ToolService,
    },
    runs::{model::LaunchDetails, service::RunService},
};
use dispatch_types::{
    AgentRunStatus, AgentRunView, AgentSandboxMode, AgentToolName, AutomationRunMutability,
};
use rootcause::{Result, prelude::*};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;
pub(crate) struct PassInput {
    pub settings: dispatch_types::ProjectSettingsView,
    pub artifact_dir: PathBuf,
    pub job_id: i64,
    pub run: AgentRunView,
    pub project: String,
    pub working: PathBuf,
    pub instructions: String,
    pub prompt: String,
    pub timeout_seconds: u64,
    pub writable: bool,
}

#[async_trait::async_trait]
pub(crate) trait PassAgent: Send + Sync {
    async fn execute(&self, shutdown: watch::Receiver<bool>, input: PassInput) -> Result<()>;
}
pub(crate) struct KnowledgePassService {
    runs: Arc<RunService>,
    codex: Arc<CodexService>,
    tools: Arc<ToolService>,
    execution: Arc<AgentExecutionService>,
    cli: Arc<crate::backend::execution::cli::CliService>,
    git: Arc<crate::backend::execution::git::GitRuntime>,
    sessions: ProcessSessionRegistry,
    files: Arc<PassFiles>,
}
impl KnowledgePassService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        runs: Arc<RunService>,
        codex: Arc<CodexService>,
        tools: Arc<ToolService>,
        execution: Arc<AgentExecutionService>,
        cli: Arc<crate::backend::execution::cli::CliService>,
        git: Arc<crate::backend::execution::git::GitRuntime>,
        sessions: ProcessSessionRegistry,
        files: Arc<PassFiles>,
    ) -> Self {
        Self {
            runs,
            codex,
            tools,
            execution,
            cli,
            git,
            sessions,
            files,
        }
    }
    async fn run(&self, shutdown: watch::Receiver<bool>, input: PassInput) -> Result<()> {
        let settings = input.settings;
        let binary = self.tools.resolve(AgentToolName::Codex).await?;
        let readiness = self.codex.readiness_for_binary(&binary).await;
        if !readiness.usable {
            bail!("Codex is unavailable: {}", readiness.message);
        }
        let dispatch_binary = self.cli.resolve().await?;
        let job_dir = input.artifact_dir.clone();
        let paths =
            self.files
                .prepare(&job_dir, input.run.id, &input.instructions, &input.prompt)?;
        let (log_dir, developer, user, log, stderr) = (
            paths.directory,
            paths.developer,
            paths.user,
            paths.log,
            paths.stderr,
        );
        let prompt = AutomationPrompt {
            developer_instructions: input.instructions,
            user_prompt: input.prompt,
        };
        let mut run = self
            .runs
            .launch(
                input.run,
                LaunchDetails {
                    work_item_id: None,
                    command: format!("{} app-server", binary.display()),
                    workspace: WorkspacePlan {
                        working_dir: input.working.clone(),
                        worktree_path: None,
                        branch_name: None,
                    },
                    developer_instructions_path: Some(developer.to_string_lossy().into_owned()),
                    user_prompt_path: Some(user.to_string_lossy().into_owned()),
                    log_path: Some(log.to_string_lossy().into_owned()),
                    agent_model: settings.default_agent_model.clone(),
                    agent_reasoning_effort: settings.default_agent_reasoning_effort,
                    commit_required: false,
                    pr_requested: false,
                    system_prompt_event_id: None,
                    effective_input_sha256: effective_input_sha256(&prompt),
                    effective_timeout_seconds: input.timeout_seconds,
                },
            )
            .await?;
        let git_runtime = self.git.prepare_git_runtime(
            run.id,
            &log_dir,
            &dispatch_binary,
            &settings,
            AutomationRunMutability::ReadOnly,
        )?;
        let real_git_path = self.git.resolve_real_git_path()?;
        let agent_id = agent_ids::dispatch_run_agent_id(run.id);
        let mut environment = self.git.agent_environment(
            &dispatch_binary,
            &git_runtime,
            &real_git_path,
            &input.project,
            &agent_id,
            None,
            Some(self.execution.api_url()),
        );
        environment.insert(
            "DISPATCH_KNOWLEDGE_LOG_DIR".into(),
            log_dir.to_string_lossy().into_owned(),
        );
        environment.insert("DISPATCH_KNOWLEDGE_JOB_ID".into(), input.job_id.to_string());
        environment.insert(
            "DISPATCH_KNOWLEDGE_PROCESS_RECORD".into(),
            job_dir
                .join(format!("process-{}.json", run.id))
                .to_string_lossy()
                .into_owned(),
        );
        // Read-only allowance accounts for isolation from the project; writable passes only edit drafts.
        let process = AgentProcessStart {
            process_record: environment
                .get("DISPATCH_KNOWLEDGE_PROCESS_RECORD")
                .map(PathBuf::from),
            incremental_log_dir: Some(log_dir.clone()),
            run_id: run.id,
            project_id: run.project_id,
            project_name: input.project,
            tool_name: AgentToolName::Codex,
            codex_binary: binary,
            codex_home: self
                .codex
                .prepare_isolated_home(&settings, &job_dir.join("codex-home"))?,
            codex_stderr_path: stderr,
            dispatch_binary,
            prompt,
            working_dir: input.working,
            git_runtime,
            real_git_path,
            agent_id,
            claimed_item_id: None,
            agent_model: settings.default_agent_model,
            agent_reasoning_effort: settings.default_agent_reasoning_effort,
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite,
            agent_extra_writable_roots: vec![],
            mutability: if input.writable {
                AutomationRunMutability::Mutating
            } else {
                AutomationRunMutability::ReadOnly
            },
            timeout: Duration::from_secs(input.timeout_seconds),
            environment: Some(environment),
        };
        let result = self.execution.execute(process, Some(shutdown)).await;
        match result {
            Ok(output) => {
                write_run_output_log(&log, &output.output)?;
                if let Some(usage) = output.token_usage {
                    run = self.runs.token_usage(run, usage).await?;
                }
                self.runs
                    .finish(
                        run,
                        AgentRunStatus::Completed,
                        Some(0),
                        output.final_response,
                    )
                    .await?;
                Ok(())
            }
            Err(error) => {
                let status = if is_automation_cancelled(&error) {
                    AgentRunStatus::Cancelled
                } else {
                    AgentRunStatus::Failed
                };
                self.runs
                    .finish(run, status, None, error.to_string())
                    .await?;
                Err(error)
            }
        }
    }
}
#[async_trait::async_trait]
impl PassAgent for KnowledgePassService {
    async fn execute(&self, shutdown: watch::Receiver<bool>, input: PassInput) -> Result<()> {
        let run_id = input.run.id;
        let marker = input.artifact_dir.join(format!("process-{run_id}.json"));
        let result = self.run(shutdown, input).await;
        let cleanup = crate::backend::execution::process_identity::cleanup(&marker).await;
        self.sessions.finish(run_id);
        cleanup?;
        result
    }
}
