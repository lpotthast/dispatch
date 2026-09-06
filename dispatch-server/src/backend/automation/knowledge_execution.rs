//! Knowledge passes use shared process execution and run logs without entering item workflows.
use super::*;

pub(crate) async fn allocate(
    store: &Store,
    project: &str,
    job_id: i64,
    timeout_seconds: u64,
) -> Result<AgentRunModel> {
    let _admission = store.lock_runtime_admission().await;
    let settings = projects::get_settings(store, project).await?;
    automation_admission::enforce_start_allowed(
        store,
        project,
        &settings,
        AutomationRunMutability::ReadOnly,
    )
    .await?;
    let run = create_run(
        store,
        settings.project_id,
        CreateRunConfig {
            tool: AgentToolName::Codex,
            mutability: AutomationRunMutability::ReadOnly,
            trigger: None,
            personality_revision_id: None,
            effective_timeout_seconds: timeout_seconds,
            effective_concurrency_group: None,
            run_kind: AgentRunKind::Task,
            purpose: AgentRunPurposeV1::KnowledgeCycle,
            knowledge_job_id: Some(job_id),
            launch_target: &AgentLaunchTargetV1::none(),
        },
    )
    .await?;
    Ok(run)
}

pub(crate) struct PassInput {
    pub job_id: i64,
    pub run: AgentRunModel,
    pub project: String,
    pub working: PathBuf,
    pub instructions: String,
    pub prompt: String,
    pub timeout_seconds: u64,
    pub writable: bool,
}

pub(crate) async fn execute(
    store: &Store,
    sessions: ProcessSessionRegistry,
    shutdown: watch::Receiver<bool>,
    input: PassInput,
) -> Result<()> {
    let run_id = input.run.id;
    let marker = crate::backend::knowledge::jobs::project_artifacts(store, input.run.project_id)
        .join(format!("jobs/{}/process-{run_id}.json", input.job_id));
    let result = execute_inner(store, sessions.clone(), shutdown, input).await;
    // The shared process wrapper terminates its owned app-server on cancellation/drop.
    let cleanup = crate::backend::knowledge::jobs::runtime::processes::cleanup(&marker).await;
    sessions.finish(run_id);
    cleanup?;
    result
}
async fn execute_inner(
    store: &Store,
    sessions: ProcessSessionRegistry,
    shutdown: watch::Receiver<bool>,
    input: PassInput,
) -> Result<()> {
    let settings = projects::get_settings(store, &input.project).await?;
    let binary = agent_tools::resolve_tool_path(store, AgentToolName::Codex).await?;
    let readiness = codex_app_server::app_server_readiness_for_binary(&binary).await;
    if !readiness.usable {
        bail!("Codex is unavailable: {}", readiness.message);
    }
    let dispatch_binary = dispatch_cli_path().await?;
    let job_dir = crate::backend::knowledge::jobs::project_artifacts(store, input.run.project_id)
        .join(format!("jobs/{}", input.job_id));
    let log_dir = job_dir.join("runs");
    fs::create_dir_all(&log_dir)?;
    let developer = log_dir.join(format!("run-{}.developer-instructions.md", input.run.id));
    let user = log_dir.join(format!("run-{}.user-prompt.md", input.run.id));
    let log = log_dir.join(format!("run-{}.output.json", input.run.id));
    let stderr = log_dir.join(format!("run-{}.codex-stderr.log", input.run.id));
    fs::write(&stderr, "")?;
    fs::write(&developer, &input.instructions)?;
    fs::write(&user, &input.prompt)?;
    let prompt = AutomationPrompt {
        developer_instructions: input.instructions,
        user_prompt: input.prompt,
    };
    let mut run = update_run_launch_details(
        store,
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
    mark_spawned_in_tx(store.db().as_ref(), run.project_id, run.id, &utc_now()).await?;
    let git_runtime = automation_runtime::prepare_git_runtime(
        run.id,
        &log_dir,
        &dispatch_binary,
        &settings,
        AutomationRunMutability::ReadOnly,
    )?;
    let real_git_path = automation_runtime::resolve_real_git_path()?;
    let agent_id = agent_ids::dispatch_run_agent_id(run.id);
    let mut environment = automation_runtime::agent_environment(
        &dispatch_binary,
        &git_runtime,
        &real_git_path,
        &input.project,
        &agent_id,
        None,
        SERVER_API_URL.get().map(String::as_str),
    );
    environment.insert(
        "DISPATCH_KNOWLEDGE_LOG_DIR".into(),
        log_dir.to_string_lossy().into_owned(),
    );
    environment.insert("DISPATCH_KNOWLEDGE_JOB_ID".into(), input.job_id.to_string());
    environment.insert(
        "DISPATCH_KNOWLEDGE_PROCESS_RECORD".into(),
        crate::backend::knowledge::jobs::project_artifacts(store, run.project_id)
            .join(format!("jobs/{}/process-{}.json", input.job_id, run.id))
            .to_string_lossy()
            .into_owned(),
    );
    // Read-only allowance accounts for isolation from the project; writable passes only edit drafts.
    let process = AgentProcessStart {
        run_id: run.id,
        project_id: run.project_id,
        project_name: input.project,
        tool_name: AgentToolName::Codex,
        codex_binary: binary,
        codex_home: codex_app_server::ensure_isolated_codex_home(
            &settings,
            &job_dir.join("codex-home"),
        )?,
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
    let result = run_agent_process(process, Some(sessions), Some(shutdown)).await;
    match result {
        Ok(output) => {
            write_run_output_log(&log, &output.output)?;
            if let Some(usage) = output.token_usage {
                run = update_run_token_usage(store, run, usage).await?;
            }
            finish_run(
                store,
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
            finish_run(store, run, status, None, error.to_string()).await?;
            Err(error)
        }
    }
}
pub(crate) async fn finish_interrupted(store: &Store, run_id: i64, message: &str) -> Result<()> {
    if let Some(run) = AgentRun::find_by_id(run_id)
        .one(store.db().as_ref())
        .await?
        && run.status == AgentRunStatus::Running.as_storage()
    {
        finish_run(store, run, AgentRunStatus::Failed, None, message.into()).await?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn finish_fake(store: &Store, run_id: i64) -> Result<()> {
    let run = AgentRun::find_by_id(run_id)
        .one(store.db().as_ref())
        .await?
        .unwrap();
    finish_run(
        store,
        run,
        AgentRunStatus::Completed,
        Some(0),
        "Deterministic fixture pass".into(),
    )
    .await?;
    Ok(())
}
