use crate::backend::execution::{
    AgentProcessStart, automation_failure_message, is_automation_cancelled,
};
use crate::backend::projects::repository::ProjectRepository;
use crate::backend::runs::model::{CreateRunConfig, LaunchDetails};

use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use rootcause::{Result, prelude::*};

use tokio_util::sync::CancellationToken;

use crate::{
    backend::{
        automation::launch::commit::CommitBaseline,
        automation::launch::prompt::{PromptContext, build_prompt, effective_input_sha256},
        automation::postconditions::model as automation_postconditions,
        execution::git::{self as automation_runtime},
        execution::identity as agent_ids,
        execution::output::{
            OutputPieceDraft, append_output_piece, new_output_piece, write_run_output_log,
        },
        execution::sessions::{ProcessSessionRegistry, ProcessSessionStart},
        projects,
        runs::launch::model::AgentLaunchTargetV1,
        storage::{TransactionManager, utc_now},
    },
    shared::view_models::{
        AgentReasoningEffort, AgentRunCleanupStatus, AgentRunKind, AgentRunOutputKind,
        AgentRunPurposeV1, AgentRunStatus, AgentRunView, AgentToolName, AutomationExecutionPolicy,
        AutomationRunMutability, PostconditionFailureView, ProjectSettingsView, ProjectView,
        SemanticPostconditionStatus, WorkItemView, WorkspaceMode, WorktreeCleanupPolicy,
    },
};

use super::{model::StartAutomation, runtime::LaunchRuntime};
use std::sync::Arc;

const AGENT_PROCESS_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
struct PreparedAutomationLaunch {
    run: AgentRunView,
    process_start: AgentProcessStart,
    log_path: PathBuf,
    commit_baseline: CommitBaseline,
}

struct LaunchPreparationInput<'a> {
    system_prompt_event_id: Option<i64>,
    personality_description: Option<&'a str>,
    project_name: &'a str,
    project: &'a ProjectView,
    settings: &'a ProjectSettingsView,
    start: &'a StartAutomation,
    tool: AgentToolName,
    run: AgentRunView,
    claimed_item: Option<&'a WorkItemView>,
    agent_id: &'a str,
    project_path: &'a Path,
    codex_binary: PathBuf,
    dispatch_binary: PathBuf,
    run_mutability: AutomationRunMutability,
}

struct LaunchPreparationFailure {
    run: Box<AgentRunView>,
    result_summary: String,
}

impl LaunchPreparationFailure {
    fn new(run: AgentRunView, result_summary: impl Into<String>) -> Self {
        Self {
            run: Box::new(run),
            result_summary: result_summary.into(),
        }
    }
}

struct StartedAutomationRun {
    project_name: String,
    system_prompt_event_id: Option<i64>,
    personality_description: Option<String>,
    project: crate::shared::view_models::ProjectView,
    settings: ProjectSettingsView,
    start: StartAutomation,
    tool: AgentToolName,
    run: AgentRunView,
}

#[derive(Clone)]
pub(crate) struct LaunchService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    items: Arc<crate::backend::items::repository::ItemRepository>,
    run_service: Arc<crate::backend::runs::service::RunService>,
    postconditions_service:
        Arc<crate::backend::automation::postconditions::service::PostconditionService>,
    personality_service:
        Arc<crate::backend::automation::personalities::service::PersonalityService>,
    codex_service: Arc<crate::backend::execution::codex::service::CodexService>,
    tool_service: Arc<crate::backend::execution::tools::service::ToolService>,
    claim_service: Arc<crate::backend::items::claims::service::ClaimService>,
    admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
    execution: Arc<crate::backend::execution::AgentExecutionService>,
    cli: Arc<crate::backend::execution::cli::CliService>,
    git: Arc<crate::backend::execution::git::GitRuntime>,
    knowledge_files: Arc<crate::backend::knowledge::runtime::KnowledgeFiles>,
    runtime: Arc<LaunchRuntime>,
    events: crate::backend::events::UiEventBus,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<crate::backend::execution::codex::model::SharedCodexStatus>,
}
impl LaunchService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        items: Arc<crate::backend::items::repository::ItemRepository>,
        run_service: Arc<crate::backend::runs::service::RunService>,
        postconditions_service: Arc<
            crate::backend::automation::postconditions::service::PostconditionService,
        >,
        personality_service: Arc<
            crate::backend::automation::personalities::service::PersonalityService,
        >,
        codex_service: Arc<crate::backend::execution::codex::service::CodexService>,
        tool_service: Arc<crate::backend::execution::tools::service::ToolService>,
        claim_service: Arc<crate::backend::items::claims::service::ClaimService>,
        admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
        execution: Arc<crate::backend::execution::AgentExecutionService>,
        cli: Arc<crate::backend::execution::cli::CliService>,
        git: Arc<crate::backend::execution::git::GitRuntime>,
        knowledge_files: Arc<crate::backend::knowledge::runtime::KnowledgeFiles>,
        runtime: Arc<LaunchRuntime>,
        events: crate::backend::events::UiEventBus,
        sessions: Option<ProcessSessionRegistry>,
        codex_status: Option<crate::backend::execution::codex::model::SharedCodexStatus>,
    ) -> Self {
        Self {
            transactions,
            projects,
            items,
            run_service,
            postconditions_service,
            personality_service,
            codex_service,
            tool_service,
            claim_service,
            admission,
            execution,
            cli,
            git,
            knowledge_files,
            runtime,
            events,
            sessions,
            codex_status,
        }
    }
    pub(crate) async fn start_until(
        &self,
        project_name: &str,
        start: StartAutomation,
        cancellation: Option<CancellationToken>,
    ) -> Result<AgentRunView> {
        let sessions = self.sessions.clone();

        let started = self.begin(project_name, start, None).await?;
        let run_id = started.run.id;
        let cancellation = register_pending_session(&started, sessions.as_ref(), cancellation);
        let service = self.clone();
        await_automation_execution(run_id, sessions, async move {
            service.complete(started, cancellation).await
        })
        .await
    }
    pub(crate) async fn start_background(
        &self,
        project_name: String,
        start: StartAutomation,
        current_item: Option<i64>,
    ) -> Result<AgentRunView> {
        let sessions = self.sessions.clone();

        let started = self.begin(&project_name, start, current_item).await?;
        let initial_run = started.run.clone();
        let run_id = started.run.id;
        let cancellation = register_pending_session(&started, sessions.as_ref(), None);
        let project_for_task = started.project_name.clone();
        let service = self.clone();
        tokio::spawn(async move {
            let result = await_automation_execution(run_id, sessions, async move {
                service.complete(started, cancellation).await
            })
            .await;
            match result {
                Ok(run) if run.status == AgentRunStatus::Failed => {
                    tracing::error!(
                        run_id = run.id,
                        project = %project_for_task,
                        summary = %run.result_summary,
                        "automation run failed"
                    );
                }
                Ok(run) if run.status == AgentRunStatus::Cancelled => {
                    tracing::warn!(
                        run_id = run.id,
                        project = %project_for_task,
                        summary = %run.result_summary,
                        "automation run cancelled"
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!(
                        project = %project_for_task,
                        error = %format_args!("{err:#}"),
                        "automation run failed"
                    );
                }
            }
        });
        Ok(initial_run)
    }
    async fn begin(
        &self,
        project_name: &str,
        mut start: StartAutomation,
        current_item: Option<i64>,
    ) -> Result<StartedAutomationRun> {
        let _admission = self.admission.acquire().await;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.by_name_in(&transaction, project_name).await?;
        if let Some(item_id) = current_item {
            let item = self.items.get_in(&transaction, project.id, item_id).await?;
            start.launch_target = AgentLaunchTargetV1::specific(item.id, item.version)?;
        }
        let settings = self
            .projects
            .settings_in(&transaction, project_name)
            .await?;
        let mutability = start
            .mutability
            .unwrap_or(AutomationRunMutability::Mutating);
        let tool = start.tool.unwrap_or(settings.default_agent_tool);
        ensure_tool_supports_mutability(tool, mutability)?;
        self.admission
            .enforce_in(
                &transaction,
                project_name,
                &settings,
                mutability,
                start.trigger.as_ref().map(|trigger| trigger.trigger_id),
                &start.execution,
            )
            .await?;
        let timeout_seconds = start
            .execution
            .timeout_seconds
            .unwrap_or(AGENT_PROCESS_TIMEOUT.as_secs());
        let personality_revision_id = self
            .personality_service
            .revision_id_in(&transaction, project.id, start.personality_id)
            .await?;
        let run = self
            .run_service
            .create_in(
                &transaction,
                project.id,
                CreateRunConfig {
                    tool,
                    mutability,
                    trigger: start.trigger.as_ref(),
                    personality_revision_id,
                    effective_timeout_seconds: timeout_seconds,
                    effective_concurrency_group: start.execution.concurrency_group.as_deref(),
                    run_kind: AgentRunKind::Task,
                    purpose: AgentRunPurposeV1::Ordinary,
                    knowledge_job_id: None,
                    launch_target: &start.launch_target,
                },
            )
            .await?;
        let system_prompt_event_id = self
            .projects
            .latest_prompt_event_id_in(&transaction, project.id)
            .await?;
        let personality_description = self
            .personality_service
            .description_for_prompt_in(&transaction, project.id, start.personality_id)
            .await?;
        transaction.commit().await?;
        self.events
            .publish_agent_run_changed(project_name, run.id, run.work_item_id);
        Ok(StartedAutomationRun {
            project_name: project_name.to_owned(),
            system_prompt_event_id,
            personality_description,
            project,
            settings,
            start,
            tool,
            run,
        })
    }
    async fn complete(
        &self,
        started: StartedAutomationRun,
        cancellation: CancellationToken,
    ) -> Result<AgentRunView> {
        let codex_status = self.codex_status.clone();

        let StartedAutomationRun {
            project_name,
            system_prompt_event_id,
            personality_description,
            project,
            settings,
            start,
            tool,
            mut run,
        } = started;
        let agent_id = agent_ids::dispatch_run_agent_id(run.id);
        let run_mutability = run.mutability;

        if cancellation.is_cancelled() {
            return self
                .run_service
                .clone()
                .finish(
                    run,
                    AgentRunStatus::Cancelled,
                    None,
                    "Automation run cancelled before startup".to_owned(),
                )
                .await;
        }

        let codex_binary = match self.tool_service.resolve(tool).await {
            Ok(codex_binary) => codex_binary,
            Err(err) => {
                return self
                    .run_service
                    .clone()
                    .finish(
                        run,
                        AgentRunStatus::Failed,
                        None,
                        format!("Failed to resolve automation tool: {err:#}"),
                    )
                    .await;
            }
        };
        let dispatch_binary = match self.cli.resolve().await {
            Ok(path) => path,
            Err(err) => {
                return self
                    .run_service
                    .clone()
                    .finish(
                        run,
                        AgentRunStatus::Failed,
                        None,
                        format!("Failed to resolve Dispatch CLI for automation: {err:#}"),
                    )
                    .await;
            }
        };
        let readiness = self.codex_service.readiness_for_binary(&codex_binary).await;
        if let Some(codex_status) = &codex_status {
            self.codex_service
                .publish_snapshot(codex_status, readiness.clone())
                .await;
        }
        if !readiness.usable {
            return self
                .run_service
                .clone()
                .finish(
                    run,
                    AgentRunStatus::Failed,
                    None,
                    format!(
                        "Codex automation preconditions failed: {}",
                        readiness.message
                    ),
                )
                .await;
        }
        if cancellation.is_cancelled() {
            return self
                .run_service
                .clone()
                .finish(
                    run,
                    AgentRunStatus::Cancelled,
                    None,
                    "Automation run cancelled before claiming work".to_owned(),
                )
                .await;
        }
        let project_path = match project
            .path
            .as_ref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
        {
            Some(path) => path,
            None => {
                return self
                    .run_service
                    .clone()
                    .finish(
                        run,
                        AgentRunStatus::Failed,
                        None,
                        format!("Failed to start automation: project '{project_name}' has no path"),
                    )
                    .await;
            }
        };

        let claimed_item = match self
            .claim_service
            .resolve_agent_run_target(
                &project_name,
                run.id,
                &agent_id,
                &start.launch_target,
                start.work_item_selector.as_ref(),
            )
            .await
        {
            Ok(claimed) => claimed,
            Err(err) => {
                return self
                    .run_service
                    .clone()
                    .finish(
                        run,
                        AgentRunStatus::Failed,
                        None,
                        format!("Failed to resolve persisted automation launch target: {err:#}"),
                    )
                    .await;
            }
        };
        if claimed_item.is_none()
            && !matches!(start.launch_target, AgentLaunchTargetV1::None { .. })
        {
            run = self
                .run_service
                .finish(
                    run,
                    AgentRunStatus::Completed,
                    None,
                    "No matching work item was available".to_owned(),
                )
                .await?;
            return Ok(run);
        }

        if cancellation.is_cancelled() {
            return self
                .run_service
                .clone()
                .finish(
                    run,
                    AgentRunStatus::Cancelled,
                    None,
                    "Automation run cancelled before launch".to_owned(),
                )
                .await;
        }

        let launch = match self
            .prepare(LaunchPreparationInput {
                system_prompt_event_id,
                personality_description: personality_description.as_deref(),
                project_name: &project_name,
                project: &project,
                settings: &settings,
                start: &start,
                tool,
                run,
                claimed_item: claimed_item.as_ref(),
                agent_id: &agent_id,
                project_path: &project_path,
                codex_binary,
                dispatch_binary,
                run_mutability,
            })
            .await
        {
            Ok(launch) => launch,
            Err(failure) => {
                return self
                    .run_service
                    .clone()
                    .finish(
                        *failure.run,
                        AgentRunStatus::Failed,
                        None,
                        failure.result_summary,
                    )
                    .await;
            }
        };
        let PreparedAutomationLaunch {
            run: prepared_run,
            process_start,
            log_path,
            commit_baseline,
        } = launch;
        run = prepared_run;

        let output = self.execution.execute(process_start, cancellation).await;
        match output {
            Ok(mut output) => {
                run = self.run_service.process_id(run, output.process_id).await?;
                if let Some(token_usage) = output.token_usage {
                    run = self.run_service.token_usage(run, token_usage).await?;
                }
                write_run_output_log(&log_path, output.output.iter()).context_with(|| {
                    format!("failed to write automation log {}", log_path.display())
                })?;
                let exit_code = Some(0);
                let mut success = true;
                let mut result_summary = if output.final_response.trim().is_empty() {
                    "Codex app-server turn completed successfully".to_owned()
                } else {
                    "Codex app-server turn completed successfully with a final response".to_owned()
                };
                let commit_evaluation = self.runtime.commits.evaluate_commit_outcome_for_run(
                    Path::new(&run.working_dir),
                    &commit_baseline,
                    run_mutability,
                );
                run = self
                    .run_service
                    .commit_outcome(run, &commit_evaluation)
                    .await?;
                if commit_evaluation.validation_failed {
                    success = false;
                    result_summary = format!(
                        "Codex app-server turn completed, but required git commit is missing: {}",
                        commit_evaluation
                            .detail
                            .as_deref()
                            .unwrap_or("no new commit was created")
                    );
                }
                let semantic = match self
                    .postconditions_service
                    .evaluate(
                        &project_name,
                        run.id,
                        claimed_item.as_ref(),
                        start.postconditions.as_ref(),
                        commit_evaluation.outcome,
                    )
                    .await
                {
                    Ok(semantic) => semantic,
                    Err(error) => automation_postconditions::SemanticEvaluation {
                        status: SemanticPostconditionStatus::Failed,
                        failures: vec![PostconditionFailureView {
                            outcome_index: 0,
                            assertion: "evaluation".to_owned(),
                            expected: "valid semantic postconditions".to_owned(),
                            actual: error.to_string(),
                        }],
                    },
                };
                run = self.run_service.semantics(run, &semantic).await?;
                if semantic.status == SemanticPostconditionStatus::Failed {
                    success = false;
                    let detail = semantic
                        .failures
                        .iter()
                        .map(|failure| {
                            format!(
                                "outcome {} {} expected {}, found {}",
                                failure.outcome_index,
                                failure.assertion,
                                failure.expected,
                                failure.actual
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    result_summary =
                        format!("{result_summary}; semantic postconditions failed: {detail}");
                    append_output_piece(
                        &mut output.output,
                        OutputPieceDraft {
                            kind: AgentRunOutputKind::Error,
                            item_id: claimed_item.as_ref().map(|item| item.id.to_string()),
                            title: "semantic postconditions failed".to_owned(),
                            body: detail,
                            metadata: serde_json::to_value(&semantic.failures)
                                .unwrap_or_else(|_| serde_json::json!([])),
                        },
                    );
                    write_run_output_log(&log_path, output.output.iter()).context_with(|| {
                        format!(
                            "failed to append semantic failure to {}",
                            log_path.display()
                        )
                    })?;
                }
                if success && pr_requested_for_run(&settings, run_mutability) {
                    match self
                        .runtime
                        .create_pull_request(Path::new(&run.working_dir))
                        .await
                    {
                        Ok(pr_url) => {
                            result_summary = format!(
                                "Codex app-server turn completed successfully; PR created: {pr_url}"
                            );
                            run = self.run_service.pull_request(run, Some(pr_url)).await?;
                        }
                        Err(err) => {
                            success = false;
                            result_summary = format!(
                                "Codex app-server turn completed, but PR creation failed: {err}"
                            );
                        }
                    }
                }

                run = self
                    .run_service
                    .finish(
                        run,
                        if success {
                            AgentRunStatus::Completed
                        } else {
                            AgentRunStatus::Failed
                        },
                        exit_code,
                        result_summary,
                    )
                    .await?;
                if success
                    && settings.worktree_cleanup_policy == WorktreeCleanupPolicy::AfterSuccess
                {
                    run = self.cleanup_run(run, &project_path).await?;
                }
                Ok(run)
            }
            Err(err) => {
                let cancelled = is_automation_cancelled(&err);
                let (message, root_cause) = if cancelled {
                    ("Automation run cancelled".to_owned(), None)
                } else {
                    let (message, root_cause) = automation_failure_message(&err);
                    (message, Some(root_cause))
                };
                let output = vec![new_output_piece(
                    1,
                    AgentRunOutputKind::Error,
                    None,
                    if cancelled { "cancelled" } else { "error" },
                    message.clone(),
                    serde_json::json!({
                        "cancelled": cancelled,
                        "root_cause": root_cause,
                    }),
                )];
                write_run_output_log(&log_path, &output).context_with(|| {
                    format!("failed to write automation log {}", log_path.display())
                })?;
                let commit_evaluation = self.runtime.commits.evaluate_commit_outcome_for_run(
                    Path::new(&run.working_dir),
                    &commit_baseline,
                    run_mutability,
                );
                run = self
                    .run_service
                    .commit_outcome(run, &commit_evaluation)
                    .await?;

                run = self
                    .run_service
                    .finish(
                        run,
                        if cancelled {
                            AgentRunStatus::Cancelled
                        } else {
                            AgentRunStatus::Failed
                        },
                        None,
                        message,
                    )
                    .await?;
                Ok(run)
            }
        }
    }
    async fn prepare(
        &self,
        input: LaunchPreparationInput<'_>,
    ) -> std::result::Result<PreparedAutomationLaunch, LaunchPreparationFailure> {
        let LaunchPreparationInput {
            system_prompt_event_id,
            personality_description,
            project_name,
            project,
            settings,
            start,
            tool,
            mut run,
            claimed_item,
            agent_id,
            project_path,
            codex_binary,
            dispatch_binary,
            run_mutability,
        } = input;

        let workspace_result = self.runtime.workspaces.prepare_workspace_for_run(
            run.id,
            project_name,
            project_path,
            settings.workspace_mode,
            run_mutability,
        );
        let workspace = match workspace_result {
            Ok(workspace) => workspace,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run,
                    format!("Failed to prepare workspace: {err}"),
                ));
            }
        };

        let log_dir = self.runtime.log_directory();
        if let Err(err) = self
            .runtime
            .prepare_directory(&log_dir)
            .context_with(|| format!("failed to create automation log dir {}", log_dir.display()))
        {
            return Err(LaunchPreparationFailure::new(
                run,
                format!("Failed to create automation log directory: {err:#}"),
            ));
        }
        let developer_instructions_path =
            log_dir.join(format!("run-{}.developer-instructions.md", run.id));
        let user_prompt_path = log_dir.join(format!("run-{}.user-prompt.md", run.id));
        let log_path = log_dir.join(format!("run-{}.output.json", run.id));
        let codex_stderr_path = log_dir.join(format!("run-{}.codex-stderr.log", run.id));
        if let Err(err) = self.runtime.write(&codex_stderr_path, "").context_with(|| {
            format!(
                "failed to prepare Codex stderr log {}",
                codex_stderr_path.display()
            )
        }) {
            return Err(LaunchPreparationFailure::new(
                run,
                format!("Failed to prepare Codex diagnostics: {err:#}"),
            ));
        }
        let agent_model = effective_agent_model(settings, claimed_item, &start.execution);
        let agent_reasoning_effort =
            effective_agent_reasoning_effort(settings, claimed_item, &start.execution);
        if let Err(err) = projects::validate_agent_model_reasoning_effort(
            "effective agent model",
            agent_model.as_deref(),
            "effective agent reasoning effort",
            agent_reasoning_effort,
        ) {
            return Err(LaunchPreparationFailure::new(
                run,
                format!("Invalid automation model configuration: {err:#}"),
            ));
        }
        let codex_home = match self.codex_service.prepare_project_home(settings) {
            Ok(codex_home) => codex_home,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run,
                    format!("Failed to prepare project Codex home: {err:#}"),
                ));
            }
        };
        let real_git_path = match self.git.resolve_real_git_path() {
            Ok(real_git_path) => real_git_path,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run,
                    format!("Failed to resolve git for automation: {err:#}"),
                ));
            }
        };
        let git_runtime = match self.git.prepare_git_runtime(
            run.id,
            &log_dir,
            &dispatch_binary,
            settings,
            run_mutability,
        ) {
            Ok(git_runtime) => git_runtime,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run,
                    format!("Failed to prepare git policy wrapper: {err:#}"),
                ));
            }
        };
        let prompt_git_policy =
            automation_runtime::git_runtime_policy_for_run(settings, run_mutability);
        let prompt_result = build_prompt(PromptContext {
            project_name,
            system_prompt: &project.system_prompt,
            item: claimed_item,
            agent_id,
            personality_description,
            extra_prompt: start.extra_prompt.as_deref(),
            mutability: run_mutability,
            workspace_mode: settings.workspace_mode,
            auto_commit: settings.auto_commit,
            commit_standard: &settings.commit_standard,
            revert_strategy: settings.revert_strategy,
            create_pr: settings.create_pr,
            git_command_policy: prompt_git_policy.policy,
            git_policy_workspace_mode: prompt_git_policy.workspace_mode,
        });
        let mut prompt = match prompt_result {
            Ok(prompt) => prompt,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run,
                    format!("Failed to build automation prompt: {err:#}"),
                ));
            }
        };
        let knowledge_context = self
            .knowledge_files
            .launch_context(
                project.id,
                workspace.working_dir.to_string_lossy().into_owned(),
                settings.knowledge_directory.clone(),
            )
            .await;
        prompt
            .developer_instructions
            .push_str("\n\n## Project Knowledge\n\n");
        prompt.developer_instructions.push_str(&knowledge_context);
        let effective_input_sha256 = effective_input_sha256(&prompt);
        if let Err(err) = self
            .runtime
            .write(&developer_instructions_path, &prompt.developer_instructions)
            .context_with(|| {
                format!(
                    "failed to write developer instructions {}",
                    developer_instructions_path.display()
                )
            })
        {
            return Err(LaunchPreparationFailure::new(
                run,
                format!("Failed to write automation developer instructions: {err:#}"),
            ));
        }
        if let Err(err) = self
            .runtime
            .write(&user_prompt_path, &prompt.user_prompt)
            .context_with(|| format!("failed to write user prompt {}", user_prompt_path.display()))
        {
            return Err(LaunchPreparationFailure::new(
                run,
                format!("Failed to write automation user prompt: {err:#}"),
            ));
        }

        let command = format!("{} app-server", codex_binary.display());
        let commit_required = commit_required_for_run(settings, run_mutability);
        let pr_requested = pr_requested_for_run(settings, run_mutability);
        let run_before_launch_update = run.clone();
        run = match self
            .run_service
            .launch(
                run,
                LaunchDetails {
                    work_item_id: claimed_item.map(|item| item.id),
                    command,
                    workspace,
                    developer_instructions_path: Some(
                        developer_instructions_path.to_string_lossy().into_owned(),
                    ),
                    user_prompt_path: Some(user_prompt_path.to_string_lossy().into_owned()),
                    log_path: Some(log_path.to_string_lossy().into_owned()),
                    agent_model: agent_model.clone(),
                    agent_reasoning_effort,
                    commit_required,
                    pr_requested,
                    system_prompt_event_id,
                    effective_input_sha256,
                    effective_timeout_seconds: start
                        .execution
                        .timeout_seconds
                        .unwrap_or(AGENT_PROCESS_TIMEOUT.as_secs()),
                },
            )
            .await
        {
            Ok(run) => run,
            Err(err) => {
                return Err(LaunchPreparationFailure::new(
                    run_before_launch_update,
                    format!("Failed to update automation launch details: {err:#}"),
                ));
            }
        };

        let commit_baseline = self
            .runtime
            .commits
            .capture_commit_baseline(Path::new(&run.working_dir), commit_required);
        let process_start = AgentProcessStart {
            process_record: None,
            incremental_log_dir: None,
            run_id: run.id,
            project_id: project.id,
            project_name: project_name.to_owned(),
            tool_name: tool,
            codex_binary,
            codex_home,
            codex_stderr_path,
            dispatch_binary,
            prompt,
            working_dir: PathBuf::from(&run.working_dir),
            git_runtime,
            real_git_path,
            agent_id: agent_id.to_owned(),
            claimed_item_id: claimed_item.map(|item| item.id),
            agent_model,
            agent_reasoning_effort,
            agent_sandbox_mode: settings.agent_sandbox_mode,
            agent_extra_writable_roots: settings.agent_extra_writable_roots.clone(),
            mutability: run_mutability,
            timeout: Duration::from_secs(
                start
                    .execution
                    .timeout_seconds
                    .unwrap_or(AGENT_PROCESS_TIMEOUT.as_secs()),
            ),
            environment: None,
        };

        Ok(PreparedAutomationLaunch {
            run,
            process_start,
            log_path,
            commit_baseline,
        })
    }
    pub(crate) async fn cleanup_worktrees(
        &self,
        project_name: &str,
        run_id: Option<i64>,
    ) -> Result<Vec<AgentRunView>> {
        let project = self.projects.by_name(project_name).await?;
        let project_id = project.id;
        let project_path = project
            .path
            .as_ref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| report!("project '{project_name}' has no path"))?;
        let runs = self
            .run_service
            .list_for_project(project_id, false, run_id)
            .await?;
        let mut cleaned = Vec::new();
        for run in runs {
            cleaned.push(
                self.run_service
                    .with_log_usage(self.cleanup_run(run, &project_path).await?)
                    .await,
            );
        }
        Ok(cleaned)
    }
    async fn cleanup_run(&self, run: AgentRunView, repo_path: &Path) -> Result<AgentRunView> {
        if run.status == AgentRunStatus::Running {
            return Ok(run);
        }
        let Some(worktree_path) = run.worktree_path.clone() else {
            return self
                .run_service
                .cleanup(run, AgentRunCleanupStatus::NotApplicable, None)
                .await;
        };
        let cleanup_status = run.cleanup_status;
        if cleanup_status == AgentRunCleanupStatus::Cleaned {
            return Ok(run);
        }
        let branch_name = run
            .branch_name
            .clone()
            .ok_or_else(|| report!("run {} has a worktree but no branch name", run.id))?;
        self.runtime.workspaces.prune_git_worktree(
            repo_path,
            &branch_name,
            Path::new(&worktree_path),
        )?;
        self.run_service
            .cleanup(run, AgentRunCleanupStatus::Cleaned, Some(utc_now()))
            .await
    }
}
fn ensure_tool_supports_mutability(
    tool: AgentToolName,
    mutability: AutomationRunMutability,
) -> Result<()> {
    match (tool, mutability) {
        (AgentToolName::Codex, AutomationRunMutability::Mutating)
        | (AgentToolName::Codex, AutomationRunMutability::ReadOnly) => Ok(()),
    }
}

async fn await_automation_execution<T, F>(
    run_id: i64,
    sessions: Option<ProcessSessionRegistry>,
    execution: F,
) -> Result<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    tokio::spawn(async move {
        let completion = ProcessSessionCompletion::new(run_id, sessions);
        let result = execution.await;
        completion.finish();
        result
    })
    .await
    .context("automation execution task terminated unexpectedly")?
}

fn register_pending_session(
    started: &StartedAutomationRun,
    sessions: Option<&ProcessSessionRegistry>,
    cancellation: Option<CancellationToken>,
) -> CancellationToken {
    let cancellation = cancellation.unwrap_or_default();
    let Some(sessions) = sessions else {
        return cancellation;
    };
    sessions
        .begin(
            ProcessSessionStart {
                run_id: started.run.id,
                project_id: started.project.id,
                project_name: started.project_name.clone(),
                tool_name: started.tool.as_storage().to_owned(),
                command: String::new(),
                working_dir: started.project.path.clone().unwrap_or_default(),
            },
            &cancellation,
        )
        .into_cancellation()
}

fn effective_agent_model(
    settings: &ProjectSettingsView,
    item: Option<&WorkItemView>,
    execution: &AutomationExecutionPolicy,
) -> Option<String> {
    item.and_then(|item| item.agent_model_override.clone())
        .or_else(|| execution.model.clone())
        .or_else(|| settings.default_agent_model.clone())
}

fn effective_agent_reasoning_effort(
    settings: &ProjectSettingsView,
    item: Option<&WorkItemView>,
    execution: &AutomationExecutionPolicy,
) -> Option<AgentReasoningEffort> {
    item.and_then(|item| item.agent_reasoning_effort_override)
        .or(execution.reasoning_effort)
        .or(settings.default_agent_reasoning_effort)
}

fn commit_required_for_policy(settings: &ProjectSettingsView) -> bool {
    match settings.workspace_mode {
        WorkspaceMode::CurrentBranch => settings.auto_commit,
        WorkspaceMode::GitBranch | WorkspaceMode::GitWorktree => true,
    }
}

fn commit_required_for_run(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> bool {
    match mutability {
        AutomationRunMutability::Mutating => commit_required_for_policy(settings),
        AutomationRunMutability::ReadOnly => false,
    }
}

fn pr_requested_for_run(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> bool {
    mutability == AutomationRunMutability::Mutating && settings.create_pr
}
struct ProcessSessionCompletion {
    run_id: i64,
    sessions: Option<ProcessSessionRegistry>,
}

impl ProcessSessionCompletion {
    fn new(run_id: i64, sessions: Option<ProcessSessionRegistry>) -> Self {
        Self { run_id, sessions }
    }

    fn finish(mut self) {
        if let Some(sessions) = &self.sessions {
            sessions.finish(self.run_id);
        }
        self.sessions = None;
    }
}

impl Drop for ProcessSessionCompletion {
    fn drop(&mut self) {
        let Some(sessions) = self.sessions.take() else {
            return;
        };
        sessions.finish(self.run_id);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
pub(crate) mod tests;
