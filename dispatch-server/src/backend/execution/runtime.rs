//! Process and protocol execution of prepared agent inputs.
use super::{
    model::{AgentProcessOutput, AgentProcessStart},
    process_identity,
};
use crate::backend::{
    execution::codex::runtime as codex_app_server,
    execution::git as automation_runtime,
    execution::output::{
        OutputPieceDraft, push_codex_output_piece, thread_event_output_piece,
        update_response_candidates, write_run_output_log,
    },
    execution::sessions::ProcessSessionRegistry,
};
use crate::shared::view_models::{
    AgentReasoningEffort, AgentRunOutputKind, AgentRunOutputPiece, AgentRunTokenUsageView,
};
use codex_app_server_sdk::{
    ApprovalMode, ClientError, StreamedTurn, Thread, ThreadEvent, ThreadOptions, TurnOptions,
};
use rootcause::{Result, prelude::*};
use std::{
    collections::HashMap,
    fmt, fs,
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

const CODEX_STREAM_RECOVERY_MAX_ATTEMPTS: usize = 12;

const CODEX_STDERR_TAIL_MAX_CHARS: usize = 12_000;

const CODEX_STREAM_RECOVERY_PROMPT: &str = "\
Dispatch recovered from a transient Codex app-server reconnect or transport interruption during \
this automation run. Continue from the existing thread context, current repository state, and \
current Dispatch item state. Do not repeat completed work; proceed to the final answer when the \
task is complete.";

#[derive(Debug)]
pub(crate) enum CodexStreamStartError {
    Spawn(Report),
    Run(ClientError),
}

impl fmt::Display for CodexStreamStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(err) => write!(f, "{err}"),
            Self::Run(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CodexStreamStartError {}

struct CodexStreamRecoveryContext<'a> {
    start: &'a AgentProcessStart,
    sessions: &'a Option<ProcessSessionRegistry>,
    env: &'a HashMap<String, String>,
    thread_options: &'a ThreadOptions,
    output: &'a mut Vec<AgentRunOutputPiece>,
}

#[derive(Debug)]
pub(crate) struct AutomationCancelled;

impl fmt::Display for AutomationCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("automation run was cancelled")
    }
}

impl std::error::Error for AutomationCancelled {}

#[derive(Debug)]
pub(crate) struct CodexAppServerStderr {
    root_cause: String,
    tail: String,
}

impl fmt::Display for CodexAppServerStderr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Codex app-server reported on stderr: {}",
            self.root_cause
        )?;
        if self.tail.trim() != self.root_cause {
            write!(f, "\nCodex app-server stderr tail:\n{}", self.tail)?;
        }
        Ok(())
    }
}

impl std::error::Error for CodexAppServerStderr {}

pub(crate) fn is_automation_cancelled(err: &Report) -> bool {
    err.iter_reports().any(|report| {
        report
            .downcast_current_context::<AutomationCancelled>()
            .is_some()
    })
}

async fn attach_codex_stderr(err: Report, stderr_path: &Path) -> Report {
    let stderr_tail = match read_codex_stderr_tail(stderr_path, CODEX_STDERR_TAIL_MAX_CHARS).await {
        Ok(stderr_tail) => stderr_tail,
        Err(read_err) if read_err.kind() == ErrorKind::NotFound => return err,
        Err(read_err) => {
            tracing::warn!(
                path = %stderr_path.display(),
                error = %read_err,
                "failed to read Codex app-server stderr diagnostics"
            );
            return err;
        }
    };
    let Some(root_cause) = codex_stderr_root_cause(&stderr_tail) else {
        return err;
    };
    let diagnostic = CodexAppServerStderr {
        root_cause,
        tail: stderr_tail,
    };
    err.context(diagnostic).into_dynamic()
}

async fn read_codex_stderr_tail(stderr_path: &Path, max_chars: usize) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(stderr_path).await?;
    let len = file.metadata().await?.len();
    let max_bytes = (max_chars as u64).saturating_mul(4);
    if len > max_bytes {
        file.seek(SeekFrom::Start(len - max_bytes)).await?;
    }

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await?;
    Ok(bounded_text_tail(
        &String::from_utf8_lossy(&bytes),
        max_chars,
    ))
}

pub(crate) fn automation_failure_message(err: &Report) -> (String, String) {
    let root_cause = err
        .iter_reports()
        .find_map(|report| {
            report
                .downcast_current_context::<CodexAppServerStderr>()
                .map(|diagnostic| diagnostic.root_cause.clone())
        })
        .or_else(|| {
            err.iter_reports()
                .last()
                .map(|report| single_line(&report.to_string()))
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "Codex app-server failed without a diagnostic message".to_owned());
    let message =
        format!("Codex automation failed. Root cause: {root_cause}\n\nTechnical details:\n{err}");
    (message, root_cause)
}

pub(crate) fn codex_stderr_root_cause(stderr: &str) -> Option<String> {
    let lines = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if let Some(caused_by) = lines
        .iter()
        .rposition(|line| line.eq_ignore_ascii_case("caused by:"))
        && let Some(line) = lines[caused_by + 1..].last()
    {
        return Some(single_line(strip_diagnostic_line_prefix(line)));
    }
    if let Some(message) = lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("Error:"))
    {
        return Some(single_line(message));
    }
    None
}

fn strip_diagnostic_line_prefix(line: &str) -> &str {
    let line = line.trim();
    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    line.get(digit_count..)
        .and_then(|rest| rest.strip_prefix(':'))
        .map(str::trim)
        .unwrap_or(line)
}

fn bounded_text_tail(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.trim().to_owned();
    }
    let tail = value
        .chars()
        .skip(char_count - max_chars)
        .collect::<String>();
    format!("[earlier stderr omitted]\n{}", tail.trim_start())
}

pub(crate) fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) struct AgentRuntime;
impl AgentRuntime {
    pub(crate) async fn execute(
        &self,
        start: AgentProcessStart,
        sessions: Option<ProcessSessionRegistry>,
        cancellation: CancellationToken,
    ) -> Result<AgentProcessOutput> {
        if let Some(registry) = &sessions {
            registry.update_command(
                start.run_id,
                format!("{} app-server", start.codex_binary.display()),
                start.working_dir.to_string_lossy().into_owned(),
            );
        }
        if cancellation.is_cancelled() {
            return Err(report!(AutomationCancelled).into_dynamic());
        }
        let codex_stderr_path = start.codex_stderr_path.clone();

        let process_timeout = start.timeout;
        let result = run_agent_process_turn_with_cancellation(
            run_codex_app_server_turn(start, sessions),
            process_timeout,
            cancellation,
        )
        .await;

        match result {
            Ok(output) => Ok(output),
            Err(err) => Err(attach_codex_stderr(err, &codex_stderr_path).await),
        }
    }
}

pub(crate) async fn run_agent_process_turn_with_cancellation(
    turn: impl std::future::Future<Output = Result<AgentProcessOutput>>,
    process_timeout: Duration,
    cancellation: CancellationToken,
) -> Result<AgentProcessOutput> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(report!(AutomationCancelled).into_dynamic()),
        result = timeout(process_timeout, turn) => {
            result.context("Codex app-server turn exceeded the automation timeout")?
        }
    }
}

async fn run_codex_app_server_turn(
    start: AgentProcessStart,
    sessions: Option<ProcessSessionRegistry>,
) -> Result<AgentProcessOutput> {
    let developer_instructions = start.prompt.developer_instructions.clone();
    let user_prompt = start.prompt.user_prompt.clone();
    let working_dir = start.working_dir.to_string_lossy().into_owned();
    let mut output = Vec::new();

    push_codex_output_piece(
        &sessions,
        start.run_id,
        &mut output,
        OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: None,
            title: "Codex app-server".to_owned(),
            body: format!(
                "starting Codex app-server from {}",
                start.codex_binary.display()
            ),
            metadata: serde_json::json!({
                "codex_binary": start.codex_binary.to_string_lossy(),
            }),
        },
    )
    .await;

    let env = start
        .environment
        .clone()
        .ok_or_else(|| report!("agent execution environment was not prepared"))?;
    let mut thread_options = ThreadOptions::builder()
        .working_directory(working_dir)
        .sandbox_mode(automation_runtime::agent_sandbox_mode_for_run(
            start.mutability,
            start.agent_sandbox_mode,
        ))
        .approval_policy(ApprovalMode::Never)
        .network_access_enabled(true)
        .sandbox_policy(automation_runtime::agent_sandbox_policy_for_run(
            start.mutability,
            start.agent_sandbox_mode,
            &start.agent_extra_writable_roots,
        ))
        .developer_instructions(developer_instructions)
        .config(automation_runtime::codex_memory_config_overrides());
    if let Some(agent_model) = start.agent_model.as_deref() {
        thread_options = thread_options.model(agent_model);
    }
    if let Some(agent_reasoning_effort) = start.agent_reasoning_effort
        && let Some(codex_reasoning_effort) =
            automation_runtime::to_codex_reasoning(agent_reasoning_effort)
    {
        thread_options = thread_options.model_reasoning_effort(codex_reasoning_effort);
    }
    let thread_options = thread_options.build();
    let (mut thread, mut streamed, mut app_server) =
        start_codex_streamed_turn(&start, &env, &thread_options, None, user_prompt)
            .await
            .map_err(|err| report!(err))
            .context("failed to start Codex app-server turn")?;
    let mut thread_id = thread.id().map(ToOwned::to_owned);

    let mut final_answer = None;
    let mut fallback_answer = None;
    let mut recovery_attempts = 0;
    let token_usage = loop {
        let event = match streamed.next_event().await {
            Some(Ok(event)) => event,
            Some(Err(err)) => {
                app_server
                    .shutdown()
                    .await
                    .context("failed to stop interrupted Codex app-server")?;
                let resumed = recover_codex_streamed_turn(
                    CodexStreamRecoveryContext {
                        start: &start,
                        sessions: &sessions,
                        env: &env,
                        thread_options: &thread_options,
                        output: &mut output,
                    },
                    thread_id.as_deref(),
                    &mut recovery_attempts,
                    err,
                )
                .await?;
                thread = resumed.0;
                streamed = resumed.1;
                app_server = resumed.2;
                thread_id = thread.id().map(ToOwned::to_owned).or(thread_id);
                continue;
            }
            None => {
                app_server
                    .shutdown()
                    .await
                    .context("failed to stop disconnected Codex app-server")?;
                let resumed = recover_codex_streamed_turn(
                    CodexStreamRecoveryContext {
                        start: &start,
                        sessions: &sessions,
                        env: &env,
                        thread_options: &thread_options,
                        output: &mut output,
                    },
                    thread_id.as_deref(),
                    &mut recovery_attempts,
                    ClientError::TransportClosed,
                )
                .await?;
                thread = resumed.0;
                streamed = resumed.1;
                app_server = resumed.2;
                thread_id = thread.id().map(ToOwned::to_owned).or(thread_id);
                continue;
            }
        };
        if let Some(piece) = thread_event_output_piece(&event) {
            push_codex_output_piece(&sessions, start.run_id, &mut output, piece).await;
            if let Some(logs) = &start.incremental_log_dir {
                use std::io::Write;
                if let Some(piece) = output.last() {
                    let mut incremental = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(logs.join(format!("run-{}.incremental.jsonl", start.run_id)))?;
                    writeln!(incremental, "{}", serde_json::to_string(piece)?)?;
                    incremental.sync_data()?;
                }
                write_run_output_log(
                    &logs.join(format!("run-{}.output.json", start.run_id)),
                    &output,
                )?;
            }
        }

        match &event {
            ThreadEvent::ThreadStarted {
                thread_id: started_thread_id,
            } => {
                thread_id = Some(started_thread_id.clone());
            }
            ThreadEvent::ItemCompleted { item } => {
                update_response_candidates(item, &mut final_answer, &mut fallback_answer);
            }
            ThreadEvent::TurnCompleted { usage } => {
                break usage.as_ref().map(|usage| AgentRunTokenUsageView {
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    output_tokens: usage.output_tokens,
                    total_tokens: usage.input_tokens.saturating_add(usage.output_tokens),
                });
            }
            ThreadEvent::TurnFailed { error } => {
                bail!("Codex app-server turn failed: {}", error.message);
            }
            ThreadEvent::Error { message } => {
                bail!("Codex app-server stream error: {message}");
            }
            ThreadEvent::TurnStarted
            | ThreadEvent::ItemStarted { .. }
            | ThreadEvent::ItemUpdated { .. } => {}
        }
    };

    app_server
        .shutdown()
        .await
        .context("failed to stop Codex app-server after the completed turn")?;

    Ok(AgentProcessOutput {
        process_id: None,
        output,
        final_response: final_answer.or(fallback_answer).unwrap_or_default(),
        token_usage,
    })
}

async fn start_codex_streamed_turn(
    start: &AgentProcessStart,
    env: &HashMap<String, String>,
    thread_options: &ThreadOptions,
    thread_id: Option<&str>,
    input: impl Into<String>,
) -> std::result::Result<
    (
        Thread,
        StreamedTurn,
        codex_app_server::ManagedCodexAppServer,
    ),
    CodexStreamStartError,
> {
    let app_server = codex_app_server::spawn_codex_with_home_and_env(
        &start.codex_binary,
        &start.codex_home,
        &start.codex_stderr_path,
        env.clone(),
    )
    .await
    .map_err(CodexStreamStartError::Spawn)?;
    if let (Some(path), Some(pid)) = (&start.process_record, app_server.process_id()) {
        process_identity::record(path, pid).map_err(CodexStreamStartError::Spawn)?;
    }
    let codex = app_server.codex();
    let options = if start.agent_reasoning_effort == Some(AgentReasoningEffort::Max) {
        thread_options_with_raw_effort(thread_options, AgentReasoningEffort::Max.as_storage())
    } else {
        thread_options.clone()
    };
    let mut thread = if let Some(thread_id) = thread_id {
        codex.resume_thread_by_id(thread_id.to_owned(), options)
    } else {
        codex.start_thread(options)
    };
    let streamed = thread
        .run_streamed(input.into(), TurnOptions::default())
        .await
        .map_err(CodexStreamStartError::Run)?;
    Ok((thread, streamed, app_server))
}

pub(crate) fn thread_options_with_raw_effort(
    options: &ThreadOptions,
    effort: &str,
) -> ThreadOptions {
    let mut options = options.clone();
    // The SDK's typed effort enum predates `max`. Keep the normal start/turn lifecycle:
    // a newly started thread has no persisted rollout and cannot be resumed yet.
    options.model_reasoning_effort = None;
    options.config.get_or_insert_default().insert(
        "model_reasoning_effort".into(),
        serde_json::Value::String(effort.into()),
    );
    options
}

async fn recover_codex_streamed_turn(
    context: CodexStreamRecoveryContext<'_>,
    thread_id: Option<&str>,
    recovery_attempts: &mut usize,
    err: ClientError,
) -> Result<(
    Thread,
    StreamedTurn,
    codex_app_server::ManagedCodexAppServer,
)> {
    if context
        .start
        .environment
        .as_ref()
        .is_some_and(|env| env.contains_key("DISPATCH_KNOWLEDGE_JOB_ID"))
    {
        return Err(report!(err)
            .context("knowledge pass transport interruption; job recovery owns retry")
            .into_dynamic());
    }
    let Some(reason) = recoverable_codex_stream_error_reason(&err) else {
        return Err(report!(err)
            .context("Codex app-server stream failed")
            .into_dynamic());
    };
    let Some(thread_id) = thread_id else {
        return Err(report!(err)
            .context("Codex app-server stream failed before a resumable thread id was available")
            .into_dynamic());
    };

    if *recovery_attempts >= CODEX_STREAM_RECOVERY_MAX_ATTEMPTS {
        bail!(
            "Codex app-server stream failed after {} recovery attempt(s): {err}",
            CODEX_STREAM_RECOVERY_MAX_ATTEMPTS
        );
    }

    loop {
        *recovery_attempts += 1;
        let attempt = *recovery_attempts;
        let backoff = codex_stream_recovery_backoff(attempt);
        push_codex_output_piece(
            context.sessions,
            context.start.run_id,
            context.output,
            codex_stream_recovery_piece(thread_id, attempt, reason, &err, backoff),
        )
        .await;
        tokio::time::sleep(backoff).await;

        match start_codex_streamed_turn(
            context.start,
            context.env,
            context.thread_options,
            Some(thread_id),
            CODEX_STREAM_RECOVERY_PROMPT,
        )
        .await
        {
            Ok(resumed) => {
                push_codex_output_piece(
                    context.sessions,
                    context.start.run_id,
                    context.output,
                    OutputPieceDraft {
                        kind: AgentRunOutputKind::System,
                        item_id: None,
                        title: "stream recovery resumed".to_owned(),
                        body: format!("resumed Codex thread {thread_id} after reconnect"),
                        metadata: serde_json::json!({
                            "thread_id": thread_id,
                            "recovery_attempt": attempt,
                            "recoverable": true,
                        }),
                    },
                )
                .await;
                return Ok(resumed);
            }
            Err(start_err) if recoverable_codex_stream_start_error(&start_err) => {
                if *recovery_attempts >= CODEX_STREAM_RECOVERY_MAX_ATTEMPTS {
                    return Err(report!(start_err)
                        .context(format!(
                            "Codex app-server stream did not recover after {} attempt(s)",
                            CODEX_STREAM_RECOVERY_MAX_ATTEMPTS
                        ))
                        .into_dynamic());
                }
                push_codex_output_piece(
                    context.sessions,
                    context.start.run_id,
                    context.output,
                    OutputPieceDraft {
                        kind: AgentRunOutputKind::System,
                        item_id: None,
                        title: "stream recovery retry".to_owned(),
                        body: format!(
                            "reconnect attempt {attempt} did not resume yet: {start_err}"
                        ),
                        metadata: serde_json::json!({
                            "thread_id": thread_id,
                            "recovery_attempt": attempt,
                            "max_recovery_attempts": CODEX_STREAM_RECOVERY_MAX_ATTEMPTS,
                            "recoverable": true,
                            "error": start_err.to_string(),
                        }),
                    },
                )
                .await;
            }
            Err(start_err) => {
                return Err(report!(start_err)
                    .context("Codex app-server stream recovery failed with a non-retryable error")
                    .into_dynamic());
            }
        }
    }
}

fn codex_stream_recovery_piece(
    thread_id: &str,
    attempt: usize,
    reason: &'static str,
    err: &ClientError,
    backoff: Duration,
) -> OutputPieceDraft {
    OutputPieceDraft {
        kind: AgentRunOutputKind::System,
        item_id: None,
        title: "recoverable stream interruption".to_owned(),
        body: format!(
            "Codex app-server stream interrupted ({reason}); reconnect attempt {attempt}/{} in {}s",
            CODEX_STREAM_RECOVERY_MAX_ATTEMPTS,
            backoff.as_secs()
        ),
        metadata: serde_json::json!({
            "thread_id": thread_id,
            "recovery_attempt": attempt,
            "max_recovery_attempts": CODEX_STREAM_RECOVERY_MAX_ATTEMPTS,
            "reason": reason,
            "recoverable": true,
            "error": err.to_string(),
        }),
    }
}

fn recoverable_codex_stream_start_error(err: &CodexStreamStartError) -> bool {
    match err {
        CodexStreamStartError::Spawn(_) => false,
        CodexStreamStartError::Run(err) => recoverable_codex_stream_error_reason(err).is_some(),
    }
}

pub(crate) fn recoverable_codex_stream_error_reason(err: &ClientError) -> Option<&'static str> {
    match err {
        ClientError::TransportClosed => Some("transport closed"),
        ClientError::TransportSend(message) if recoverable_transport_message(message) => {
            Some("transport send failed")
        }
        ClientError::Io(err) if recoverable_transport_io_error(err.kind()) => {
            Some("transport I/O interrupted")
        }
        ClientError::Timeout { .. } => Some("request timed out"),
        ClientError::Rpc { error } if recoverable_rpc_message(&error.message) => {
            Some("turn still active during reconnect")
        }
        ClientError::NotInitialized { .. }
        | ClientError::NotReady { .. }
        | ClientError::AlreadyInitialized
        | ClientError::TransportSend(_)
        | ClientError::InvalidMessage(_)
        | ClientError::Serialization(_)
        | ClientError::Io(_)
        | ClientError::Rpc { .. }
        | ClientError::UnexpectedResult { .. } => None,
    }
}

pub(crate) fn recoverable_transport_io_error(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::Interrupted
            | ErrorKind::NotConnected
            | ErrorKind::TimedOut
            | ErrorKind::UnexpectedEof
    )
}

pub(crate) fn recoverable_transport_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "event channel receive failed",
        "failed to send outbound frame",
        "transport closed",
        "connection aborted",
        "connection closed",
        "connection lost",
        "connection reset",
        "broken pipe",
        "channel closed",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub(crate) fn recoverable_rpc_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "active turn",
        "turn already",
        "turn is already",
        "turn is still",
        "currently running",
        "in progress",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub(crate) fn codex_stream_recovery_backoff(attempt: usize) -> Duration {
    let seconds = match attempt {
        0 | 1 => 2,
        2 => 5,
        3 => 10,
        4 => 20,
        _ => 30,
    };
    Duration::from_secs(seconds)
}

pub(crate) fn automation_log_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".dispatch").join("runs");
    }
    PathBuf::from(".dispatch").join("runs")
}

#[cfg(test)]
mod tests {
    #[test]
    fn codex_stream_recovery_logs_concise_system_note() {
        let piece = codex_stream_recovery_piece(
            "thread-1",
            1,
            "transport closed",
            &ClientError::TransportClosed,
            Duration::from_secs(2),
        );
        assert_that!(&piece.kind).is_equal_to(AgentRunOutputKind::System);
        assert_that!(&piece.title).is_equal_to("recoverable stream interruption");
        assert_that!(&piece.body.contains("reconnect attempt 1/")).is_true();
        assert_that!(&piece.metadata["recoverable"]).is_equal_to(true);
        assert_that!(&piece.metadata["thread_id"]).is_equal_to("thread-1");
    }

    use super::*;
    use assertr::prelude::*;
    use tempfile::TempDir;
    const AGENT_PROCESS_TIMEOUT: Duration = Duration::from_secs(12 * 60 * 60);
    #[test]
    fn raw_effort_thread_options_preserve_launch_options() {
        let options = ThreadOptions::builder()
            .working_directory("/tmp/dispatch-work")
            .approval_policy(ApprovalMode::Never)
            .sandbox_mode(codex_app_server_sdk::SandboxMode::WorkspaceWrite)
            .network_access_enabled(true)
            .developer_instructions("Use Dispatch policy.")
            .config(serde_json::Map::from_iter([(
                "features.memories".to_owned(),
                serde_json::Value::Bool(false),
            )]))
            .build();

        let params = thread_options_with_raw_effort(&options, "max");

        assert_that!(&(params.working_directory.as_deref()))
            .is_equal_to(Some("/tmp/dispatch-work"));
        assert_that!(&(params.approval_policy)).is_equal_to(Some(ApprovalMode::Never));
        assert_that!(&(params.sandbox_mode))
            .is_equal_to(Some(codex_app_server_sdk::SandboxMode::WorkspaceWrite));
        assert_that!(
            &params
                .config
                .as_ref()
                .unwrap()
                .get("model_reasoning_effort")
        )
        .is_equal_to(Some(&serde_json::json!("max")));
        assert_that!(&(params.developer_instructions.as_deref()))
            .is_equal_to(Some("Use Dispatch policy."));
        assert_that!(&params.network_access_enabled).is_equal_to(Some(true));
        assert_that!(&params.config.as_ref().unwrap().get("features.memories"))
            .is_equal_to(Some(&serde_json::Value::Bool(false)));
    }

    #[test]
    fn codex_stream_recovery_classifies_transport_interruptions() {
        assert_that!(&(recoverable_codex_stream_error_reason(&ClientError::TransportClosed)))
            .is_equal_to(Some("transport closed"));
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::TransportSend(
                "event channel receive failed: channel closed".to_owned()
            )))
        )
        .is_equal_to(Some("transport send failed"));
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::Io(std::io::Error::from(
                ErrorKind::BrokenPipe
            ))))
        )
        .is_equal_to(Some("transport I/O interrupted"));
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::Timeout {
                method: "turn/start".to_owned(),
                timeout_ms: 30_000,
            }))
        )
        .is_equal_to(Some("request timed out"));
    }

    #[test]
    fn codex_stream_recovery_leaves_non_retryable_errors_terminal() {
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::TransportSend(
                "thread id unavailable after start/resume".to_owned()
            )))
        )
        .is_equal_to(None);
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::InvalidMessage(
                "expected JSON object".to_owned()
            )))
        )
        .is_equal_to(None);
        assert_that!(
            &(recoverable_codex_stream_error_reason(&ClientError::Rpc {
                error: codex_app_server_sdk::RpcError {
                    code: -32_000,
                    message: "model rejected the request".to_owned(),
                    data: None,
                },
            }))
        )
        .is_equal_to(None);
    }

    #[test]
    fn codex_stderr_prefers_the_reported_root_cause() {
        let stderr = "2026-07-10T13:08:04Z ERROR stale state path\n\
                      Error: No such file or directory (os error 2)\n";

        assert_that!(&(codex_stderr_root_cause(stderr).as_deref()))
            .is_equal_to(Some("No such file or directory (os error 2)"));
    }

    #[test]
    fn codex_stderr_uses_the_deepest_chained_cause() {
        let stderr = "Error: failed to initialize Codex state\n\n\
                      Caused by:\n\
                        0: failed to open state database\n\
                        1: Permission denied (os error 13)\n";

        assert_that!(&(codex_stderr_root_cause(stderr).as_deref()))
            .is_equal_to(Some("Permission denied (os error 13)"));
    }

    #[test]
    fn codex_stderr_does_not_promote_ordinary_error_logs() {
        let stderr = "2026-07-10T13:08:04Z ERROR codex_rollout::list: stale state path\n";

        assert_that!(&(codex_stderr_root_cause(stderr))).is_equal_to(None);
    }

    #[tokio::test]
    async fn codex_stderr_tail_reader_bounds_large_logs() {
        let temp = TempDir::new().unwrap();
        let stderr_path = temp.path().join("codex.stderr.log");
        let body = format!(
            "{}Error: Permission denied (os error 13)\n",
            "background diagnostic line\n".repeat(256)
        );
        tokio::fs::write(&stderr_path, &body).await.unwrap();

        let tail = read_codex_stderr_tail(&stderr_path, 96).await.unwrap();

        assert_that!(&(tail.starts_with("[earlier stderr omitted]"))).is_true();
        assert_that!(&(tail.len() < body.len())).is_true();
        assert_that!(&(tail.contains("Error: Permission denied (os error 13)"))).is_true();
        assert_that!(&(codex_stderr_root_cause(&tail).as_deref()))
            .is_equal_to(Some("Permission denied (os error 13)"));
    }

    #[tokio::test]
    async fn automation_failure_highlights_captured_codex_stderr() {
        let temp = TempDir::new().unwrap();
        let stderr_path = temp.path().join("codex.stderr.log");
        tokio::fs::write(
            &stderr_path,
            "Error: No such file or directory (os error 2)\n",
        )
        .await
        .unwrap();
        let err = report!(ClientError::Rpc {
            error: codex_app_server_sdk::RpcError {
                code: -32_098,
                message: "transport error: transport closed".to_owned(),
                data: None,
            },
        })
        .context("failed to start Codex app-server turn")
        .into_dynamic();

        let err = attach_codex_stderr(err, &stderr_path).await;
        let (message, root_cause) = automation_failure_message(&err);

        assert_that!(&(root_cause)).is_equal_to("No such file or directory (os error 2)");
        assert_that!(
            &(message.starts_with(
                "Codex automation failed. Root cause: No such file or directory (os error 2)"
            ))
        )
        .is_true();
        assert_that!(&(message.contains("transport error: transport closed"))).is_true();
        assert_that!(&(message.contains("Codex app-server reported on stderr"))).is_true();
    }

    #[tokio::test]
    async fn cancelled_run_does_not_poll_the_agent_turn() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let polled = std::cell::Cell::new(false);
        let err = run_agent_process_turn_with_cancellation(
            async {
                polled.set(true);
                bail!("agent turn must not start")
            },
            AGENT_PROCESS_TIMEOUT,
            cancellation,
        )
        .await
        .unwrap_err();

        assert_that!(&is_automation_cancelled(&err)).is_true();
        assert_that!(&polled.get()).is_false();
    }

    #[tokio::test]
    async fn uncancelled_turn_keeps_its_timeout() {
        let err = run_agent_process_turn_with_cancellation(
            std::future::pending(),
            Duration::ZERO,
            CancellationToken::new(),
        )
        .await
        .unwrap_err();

        assert_that!(&is_automation_cancelled(&err)).is_false();
        assert_that!(&err.to_string()).contains("exceeded the automation timeout");
    }

    #[tokio::test]
    async fn explicit_cancellation_still_cancels_waiting_turn() {
        let cancellation = CancellationToken::new();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_agent_process_turn_with_cancellation(
            async move {
                let _ = started.send(());
                std::future::pending::<Result<AgentProcessOutput>>().await
            },
            AGENT_PROCESS_TIMEOUT,
            cancellation.clone(),
        ));

        tokio::time::timeout(Duration::from_secs(1), ready)
            .await
            .unwrap()
            .unwrap();
        cancellation.cancel();
        let err = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();

        assert_that!(&(is_automation_cancelled(&err))).is_true();
    }

    #[tokio::test]
    async fn non_retryable_turn_error_still_fails() {
        let err = run_agent_process_turn_with_cancellation(
            async { bail!("permanent Codex SDK failure") },
            AGENT_PROCESS_TIMEOUT,
            CancellationToken::new(),
        )
        .await
        .unwrap_err();

        assert_that!(&(!is_automation_cancelled(&err))).is_true();
        assert_that!(&(err.to_string().contains("permanent Codex SDK failure"))).is_true();
    }
}
