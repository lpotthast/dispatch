//! Codex app-server readiness checks and Dispatch-managed runtime configuration.
//!
//! Dispatch distinguishes an app-server that can be started (`available`) from one whose account
//! and rate-limit state permit automation (`usable`). This module owns that readiness probe, the
//! optional detailed usage probe, the shared and per-project Codex homes, and the owned WebSocket
//! app-server processes used by probes and automation runs.
//!
//! The SDK currently exposes the account-related responses as opaque JSON objects. The private
//! wire types below mirror the app-server schema so incompatible shape or field-type changes fail
//! at one documented deserialization boundary instead of leaking stringly typed JSON traversal
//! through the module. Unknown enum values remain intact for forward-compatible status output.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use codex_app_server_sdk::{
    ClientError, Codex, CodexClient, WsConfig,
    requests::{ClientInfo, GetAccountParams, InitializeParams},
};
use rootcause::{Result, prelude::*};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{
    net::TcpListener,
    process::Command,
    sync::oneshot,
    task::JoinHandle,
    time::{Instant, sleep, timeout},
};
use tokio_process_tools::{
    Consumable, DEFAULT_MAX_BUFFERED_CHUNKS, DEFAULT_READ_CHUNK_SIZE, GracefulShutdown, Process,
    ProcessStreamBuilder, WaitForCompletionResult, WriteCollectionOptions,
    visitors::write::WriteChunks,
};

use crate::shared::view_models::{
    AgentSandboxMode, CodexAppServerStatusView, CodexAuthSetupView, CodexLogStorageStatusView,
    CodexPreconditionView, CodexRateLimitView, CodexUsageSummaryView, ProjectSettingsView,
};

pub(super) const STATUS_TIMEOUT: Duration = Duration::from_secs(14);
pub(super) const USAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const APP_SERVER_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const APP_SERVER_CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(25);
pub(super) const APP_SERVER_EXIT_POLL_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const APP_SERVER_LOG_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const APP_SERVER_STATUS_LOG_FILE: &str = "status-app-server.log";
// The SDK has no typed account-usage method, so raw protocol access is isolated to
// `read_usage_response` below.
pub(super) const ACCOUNT_USAGE_READ_METHOD: &str = "account/usage/read";
pub(super) const CLIENT_NAME: &str = "dispatch";
pub(super) const CLIENT_TITLE: &str = "Dispatch";
pub(super) const CODEX_HOME_DIR: &str = "codex";
pub(super) const CODEX_CONFIG: &str = r#"# Managed by Dispatch.
# Dispatch provides canonical project guidance through its knowledge service and
# keeps Codex memories and optional remote catalogs disabled for deterministic,
# low-traffic runs. Dispatch manages Codex updates separately from agent startup.

check_for_update_on_startup = false

[features]
apps = false
memories = false
remote_plugin = false

[memories]
use_memories = false
generate_memories = false
disable_on_external_context = true
"#;
pub(super) const PROJECT_RULES_FILE_NAME: &str = "dispatch-git.rules";

pub(super) struct AppServerProcess {
    pid: Option<u32>,
    shutdown: Option<oneshot::Sender<()>>,
    monitor: Option<JoinHandle<Result<()>>>,
}

impl AppServerProcess {
    fn is_finished(&self) -> bool {
        self.monitor.as_ref().is_none_or(JoinHandle::is_finished)
    }

    async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.join_monitor().await
    }

    async fn join_monitor(mut self) -> Result<()> {
        let Some(monitor) = self.monitor.take() else {
            return Ok(());
        };
        monitor
            .await
            .context("failed to join Codex app-server process monitor")?
    }
}

impl Drop for AppServerProcess {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

/// A Codex protocol client paired with the Dispatch-owned app-server process serving it.
///
/// Dropping the owner requests process termination. Call [`Self::shutdown`] when the caller needs
/// deterministic confirmation that the process and its output collector have exited.
pub(crate) struct ManagedCodexAppServer {
    client: CodexClient,
    process: AppServerProcess,
}

impl ManagedCodexAppServer {
    pub(crate) fn process_id(&self) -> Option<u32> {
        self.process.pid
    }
    pub(super) fn client(&self) -> &CodexClient {
        &self.client
    }

    pub(crate) fn codex(&self) -> Codex {
        Codex::from_client(self.client.clone())
    }

    pub(crate) async fn shutdown(self) -> Result<()> {
        self.process.shutdown().await
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum StatusProbe {
    Readiness,
    Detailed,
}

impl StatusProbe {
    fn includes_usage(self) -> bool {
        self == Self::Detailed
    }
}

/// Operator-facing guidance shown when the Codex executable cannot be discovered or started.
pub const CODEX_INSTALL_PROMPT: &str =
    "Install Codex and make sure `codex app-server` is available on PATH.";

/// Typed representation of the opaque `account/read` SDK response.
///
/// Fields remain optional where older app-server versions omitted them. Missing authentication
/// state is handled conservatively by the readiness check.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountReadResponse {
    #[serde(default)]
    account: Option<CodexAccount>,
    #[serde(default)]
    requires_openai_auth: Option<bool>,
}

/// Account details returned by the Codex app-server protocol.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CodexAccount {
    #[serde(rename = "type")]
    auth_method: CodexAuthMethod,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    plan_type: Option<CodexPlanType>,
}

impl CodexAccount {
    fn auth_method(&self) -> &str {
        self.auth_method.as_str()
    }

    fn label(&self) -> Option<String> {
        match &self.auth_method {
            CodexAuthMethod::ApiKey => Some("API key".to_owned()),
            CodexAuthMethod::ChatGpt => self.email.clone(),
            CodexAuthMethod::AmazonBedrock => Some("Amazon Bedrock".to_owned()),
            CodexAuthMethod::Other(method) => Some(method.clone()),
        }
    }

    fn plan_type(&self) -> Option<CodexPlanType> {
        if matches!(&self.auth_method, CodexAuthMethod::ChatGpt) {
            self.plan_type.clone()
        } else {
            None
        }
    }
}

/// Authentication methods currently defined by the app-server schema.
///
/// Unknown methods retain their wire value so Dispatch can surface a new provider without first
/// requiring a release that knows its name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub(super) enum CodexAuthMethod {
    ApiKey,
    ChatGpt,
    AmazonBedrock,
    Other(String),
}

impl CodexAuthMethod {
    fn as_str(&self) -> &str {
        match self {
            Self::ApiKey => "apiKey",
            Self::ChatGpt => "chatgpt",
            Self::AmazonBedrock => "amazonBedrock",
            Self::Other(method) => method,
        }
    }
}

impl From<String> for CodexAuthMethod {
    fn from(method: String) -> Self {
        match method.as_str() {
            "apiKey" => Self::ApiKey,
            "chatgpt" => Self::ChatGpt,
            "amazonBedrock" => Self::AmazonBedrock,
            _ => Self::Other(method),
        }
    }
}

/// Subscription plan values currently defined by the app-server schema.
///
/// Unknown plans retain their wire value for forward-compatible status reporting.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub(super) enum CodexPlanType {
    Free,
    Go,
    Plus,
    Pro,
    Prolite,
    Team,
    SelfServeBusinessUsageBased,
    Business,
    EnterpriseCbpUsageBased,
    Enterprise,
    Edu,
    Unknown,
    Other(String),
}

impl CodexPlanType {
    fn as_str(&self) -> &str {
        match self {
            Self::Free => "free",
            Self::Go => "go",
            Self::Plus => "plus",
            Self::Pro => "pro",
            Self::Prolite => "prolite",
            Self::Team => "team",
            Self::SelfServeBusinessUsageBased => "self_serve_business_usage_based",
            Self::Business => "business",
            Self::EnterpriseCbpUsageBased => "enterprise_cbp_usage_based",
            Self::Enterprise => "enterprise",
            Self::Edu => "edu",
            Self::Unknown => "unknown",
            Self::Other(plan_type) => plan_type,
        }
    }

    fn is_usage_based(&self) -> bool {
        matches!(
            self,
            Self::SelfServeBusinessUsageBased | Self::EnterpriseCbpUsageBased
        )
    }
}

impl From<String> for CodexPlanType {
    fn from(plan_type: String) -> Self {
        match plan_type.as_str() {
            "free" => Self::Free,
            "go" => Self::Go,
            "plus" => Self::Plus,
            "pro" => Self::Pro,
            "prolite" => Self::Prolite,
            "team" => Self::Team,
            "self_serve_business_usage_based" => Self::SelfServeBusinessUsageBased,
            "business" => Self::Business,
            "enterprise_cbp_usage_based" => Self::EnterpriseCbpUsageBased,
            "enterprise" => Self::Enterprise,
            "edu" => Self::Edu,
            "unknown" => Self::Unknown,
            _ => Self::Other(plan_type),
        }
    }
}

/// Typed representation of the opaque `account/rateLimits/read` SDK response.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RateLimitsReadResponse {
    #[serde(default)]
    rate_limits: Option<RateLimitSnapshot>,
    #[serde(default)]
    rate_limits_by_limit_id: Option<BTreeMap<String, RateLimitSnapshot>>,
}

impl RateLimitsReadResponse {
    fn plan_type(&self) -> Option<CodexPlanType> {
        self.rate_limits_by_limit_id
            .as_ref()
            .into_iter()
            .flat_map(|limits| limits.values())
            .chain(self.rate_limits.iter())
            .find_map(|limit| limit.plan_type.clone())
    }

    fn into_views(self) -> Vec<CodexRateLimitView> {
        if let Some(limits) = self
            .rate_limits_by_limit_id
            .filter(|limits| !limits.is_empty())
        {
            return limits
                .into_iter()
                .map(|(limit_id, snapshot)| snapshot.into_view(Some(limit_id)))
                .collect();
        }

        self.rate_limits
            .map(|snapshot| vec![snapshot.into_view(None)])
            .unwrap_or_default()
    }
}

/// One rate-limit bucket returned by Codex.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RateLimitSnapshot {
    #[serde(default)]
    limit_id: Option<String>,
    #[serde(default)]
    limit_name: Option<String>,
    #[serde(default)]
    plan_type: Option<CodexPlanType>,
    #[serde(default)]
    primary: Option<RateLimitWindow>,
    #[serde(default)]
    secondary: Option<RateLimitWindow>,
    #[serde(default)]
    individual_limit: Option<SpendControlLimit>,
    #[serde(default)]
    credits: Option<CreditsSnapshot>,
    #[serde(default)]
    rate_limit_reached_type: Option<RateLimitReachedType>,
}

impl RateLimitSnapshot {
    fn into_view(self, fallback_label: Option<String>) -> CodexRateLimitView {
        let primary = self.primary.unwrap_or_default();
        let secondary = self.secondary.unwrap_or_default();
        let individual = self.individual_limit.unwrap_or_default();
        let credits = self.credits.unwrap_or_default();

        CodexRateLimitView {
            label: self
                .limit_name
                .or(self.limit_id)
                .or(fallback_label)
                .unwrap_or_else(|| "Codex".to_owned()),
            plan_type: self
                .plan_type
                .map(|plan_type| plan_type.as_str().to_owned()),
            primary_used_percent: primary.used_percent,
            primary_window_minutes: primary.window_duration_mins,
            primary_resets_at: primary.resets_at.and_then(format_unix_timestamp),
            secondary_used_percent: secondary.used_percent,
            secondary_window_minutes: secondary.window_duration_mins,
            secondary_resets_at: secondary.resets_at.and_then(format_unix_timestamp),
            individual_used: individual.used,
            individual_limit: individual.limit,
            individual_remaining_percent: individual.remaining_percent,
            individual_resets_at: individual.resets_at.and_then(format_unix_timestamp),
            credits_balance: credits.balance,
            credits_has_credits: credits.has_credits,
            credits_unlimited: credits.unlimited,
            reached_type: self
                .rate_limit_reached_type
                .map(|reached| reached.as_str().to_owned()),
        }
    }
}

/// Percentage and reset metadata for one rolling usage window.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RateLimitWindow {
    #[serde(default)]
    used_percent: Option<i64>,
    #[serde(default)]
    window_duration_mins: Option<i64>,
    #[serde(default)]
    resets_at: Option<i64>,
}

/// Optional workspace spend-control values attached to a rate-limit bucket.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SpendControlLimit {
    #[serde(default)]
    used: Option<String>,
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    remaining_percent: Option<i64>,
    #[serde(default)]
    resets_at: Option<i64>,
}

/// Credit-balance metadata attached to a rate-limit bucket.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreditsSnapshot {
    #[serde(default)]
    balance: Option<String>,
    #[serde(default)]
    has_credits: Option<bool>,
    #[serde(default)]
    unlimited: Option<bool>,
}

/// Reasons the app-server reports a rate-limit bucket as blocked.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub(super) enum RateLimitReachedType {
    RateLimitReached,
    WorkspaceOwnerCreditsDepleted,
    WorkspaceMemberCreditsDepleted,
    WorkspaceOwnerUsageLimitReached,
    WorkspaceMemberUsageLimitReached,
    Other(String),
}

impl RateLimitReachedType {
    fn as_str(&self) -> &str {
        match self {
            Self::RateLimitReached => "rate_limit_reached",
            Self::WorkspaceOwnerCreditsDepleted => "workspace_owner_credits_depleted",
            Self::WorkspaceMemberCreditsDepleted => "workspace_member_credits_depleted",
            Self::WorkspaceOwnerUsageLimitReached => "workspace_owner_usage_limit_reached",
            Self::WorkspaceMemberUsageLimitReached => "workspace_member_usage_limit_reached",
            Self::Other(reached_type) => reached_type,
        }
    }
}

impl From<String> for RateLimitReachedType {
    fn from(reached_type: String) -> Self {
        match reached_type.as_str() {
            "rate_limit_reached" => Self::RateLimitReached,
            "workspace_owner_credits_depleted" => Self::WorkspaceOwnerCreditsDepleted,
            "workspace_member_credits_depleted" => Self::WorkspaceMemberCreditsDepleted,
            "workspace_owner_usage_limit_reached" => Self::WorkspaceOwnerUsageLimitReached,
            "workspace_member_usage_limit_reached" => Self::WorkspaceMemberUsageLimitReached,
            _ => Self::Other(reached_type),
        }
    }
}

/// Typed response for the raw `account/usage/read` request.
#[derive(Debug, Deserialize)]
pub(super) struct UsageReadResponse {
    #[serde(default)]
    summary: Option<UsageSummary>,
}

/// Aggregate token-usage values displayed by Dispatch.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UsageSummary {
    #[serde(default)]
    lifetime_tokens: Option<i64>,
    #[serde(default)]
    peak_daily_tokens: Option<i64>,
    #[serde(default)]
    current_streak_days: Option<i64>,
    #[serde(default)]
    longest_streak_days: Option<i64>,
    #[serde(default)]
    longest_running_turn_sec: Option<i64>,
}

impl From<UsageSummary> for CodexUsageSummaryView {
    fn from(summary: UsageSummary) -> Self {
        Self {
            lifetime_tokens: summary.lifetime_tokens,
            peak_daily_tokens: summary.peak_daily_tokens,
            current_streak_days: summary.current_streak_days,
            longest_streak_days: summary.longest_streak_days,
            longest_running_turn_seconds: summary.longest_running_turn_sec,
        }
    }
}

/// Dispatch-owned entries linked from the shared Codex home into project homes.
#[derive(Debug, Clone, Copy)]
pub(super) enum SharedCodexEntry {
    Auth,
    InstallationId,
    Skills,
}

impl SharedCodexEntry {
    fn file_name(self) -> &'static str {
        match self {
            Self::Auth => "auth.json",
            Self::InstallationId => "installation_id",
            Self::Skills => "skills",
        }
    }
}

/// Decisions supported by Codex prefix rules generated by Dispatch.
#[derive(Debug, Clone, Copy)]
pub(super) enum RuleDecision {
    Allow,
    Forbidden,
}

impl RuleDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Forbidden => "forbidden",
        }
    }
}

/// Readiness conditions presented to operators and enforced before automation starts.
#[derive(Debug, Clone, Copy)]
pub(super) enum ReadinessPrecondition {
    AppServer,
    Account,
    UsageLimits,
}

impl ReadinessPrecondition {
    fn view(self, ok: bool, message: impl Into<String>) -> CodexPreconditionView {
        CodexPreconditionView {
            name: self.name().to_owned(),
            ok,
            message: message.into(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::AppServer => "Codex app-server",
            Self::Account => "Codex account",
            Self::UsageLimits => "Codex usage limits",
        }
    }
}

/// Authenticated account operations that can reveal invalidated credentials.
#[derive(Debug, Clone, Copy)]
pub(super) enum AccountOperation {
    AccountStatus,
    RateLimits,
    TokenUsage,
}

impl AccountOperation {
    fn description(self) -> &'static str {
        match self {
            Self::AccountStatus => "reading account status",
            Self::RateLimits => "reading rate limits",
            Self::TokenUsage => "reading token usage",
        }
    }
}

pub(super) async fn app_server_status_for_binary_unlocked(
    codex_home: &Path,
    codex_binary: &Path,
    checked_at: String,
    probe: StatusProbe,
) -> CodexAppServerStatusView {
    match timeout(
        STATUS_TIMEOUT,
        inspect_app_server(codex_home, codex_binary, checked_at.clone(), probe),
    )
    .await
    {
        Ok(status) => status,
        Err(_) => unavailable_status(
            checked_at,
            format!(
                "Codex app-server is unavailable: timed out while checking `{}`.",
                codex_binary.display()
            ),
        ),
    }
}

pub(super) async fn inspect_app_server(
    codex_home: &Path,
    codex_binary: &Path,
    checked_at: String,
    probe: StatusProbe,
) -> CodexAppServerStatusView {
    let app_server = match spawn_initialized_app_server(codex_home, codex_binary).await {
        Ok(app_server) => app_server,
        Err(err) => {
            return unavailable_status(
                checked_at,
                format!(
                    "Codex app-server is unavailable: failed to initialize `{}`: {err:#}",
                    codex_binary.display()
                ),
            );
        }
    };

    let mut status = inspect_initialized_client(
        codex_home,
        codex_binary,
        checked_at,
        app_server.client(),
        probe,
    )
    .await;
    if let Err(err) = app_server.shutdown().await {
        status.warnings.push(format!(
            "Codex app-server process cleanup could not be confirmed: {err:#}"
        ));
    }
    status
}

pub(super) async fn inspect_initialized_client(
    codex_home: &Path,
    codex_binary: &Path,
    checked_at: String,
    client: &CodexClient,
    probe: StatusProbe,
) -> CodexAppServerStatusView {
    let binary_path = codex_binary.to_string_lossy().into_owned();

    let mut preconditions = vec![
        ReadinessPrecondition::AppServer
            .view(true, format!("Initialized `{}`.", codex_binary.display())),
    ];
    let mut warnings = Vec::new();

    let account_response = match read_account_response(client).await {
        Ok(response) => response,
        Err(err) => {
            let message = auth_failure_message(codex_home, AccountOperation::AccountStatus, &err)
                .unwrap_or_else(|| format!("Codex account status could not be read: {err}"));
            let auth_setup = codex_auth_setup(codex_home, codex_binary);
            preconditions.push(ReadinessPrecondition::Account.view(false, message.clone()));
            return CodexAppServerStatusView {
                available: true,
                usable: false,
                message: format!("Codex SDK is unusable for automation. {message}"),
                install_prompt: CODEX_INSTALL_PROMPT.to_owned(),
                auth_setup: Some(auth_setup),
                checked_at,
                binary_path: Some(binary_path),
                requires_openai_auth: None,
                signed_in: false,
                auth_method: None,
                account_label: None,
                plan_type: None,
                payment_model: None,
                preconditions,
                rate_limits: Vec::new(),
                usage_summary: None,
                warnings,
                log_storage: CodexLogStorageStatusView::default(),
            };
        }
    };

    let requires_openai_auth = account_response.requires_openai_auth;
    let account = account_response.account.as_ref();
    let auth_method = account.map(|account| account.auth_method().to_owned());
    let account_label = account.and_then(CodexAccount::label);
    let signed_in = account.is_some();
    let mut account_ok = signed_in || requires_openai_auth == Some(false);
    let mut account_message = account_status_message(
        codex_home,
        account,
        account_label.as_deref(),
        requires_openai_auth,
    );
    let mut auth_failure = None::<String>;

    let rate_limit_response = match read_rate_limits_response(client).await {
        Ok(response) => Some(response),
        Err(err) => {
            if let Some(message) =
                auth_failure_message(codex_home, AccountOperation::RateLimits, &err)
            {
                auth_failure.get_or_insert(message);
            } else {
                warnings.push(format!("Codex rate limits could not be read: {err}"));
            }
            None
        }
    };
    let rate_limit_plan_type = rate_limit_response
        .as_ref()
        .and_then(RateLimitsReadResponse::plan_type);
    let mut rate_limits = rate_limit_response
        .map(RateLimitsReadResponse::into_views)
        .unwrap_or_default();
    rate_limits.sort_by(|left, right| left.label.cmp(&right.label));
    let plan_type = account
        .and_then(CodexAccount::plan_type)
        .or(rate_limit_plan_type);
    let reached = rate_limits
        .iter()
        .filter_map(|limit| limit.reached_type.as_deref())
        .collect::<Vec<_>>();

    let usage_summary = if probe.includes_usage() {
        match read_usage_response(client).await {
            Ok(response) => match response.summary {
                Some(summary) => Some(summary.into()),
                None => {
                    warnings
                        .push("Codex token usage response did not include a summary.".to_owned());
                    None
                }
            },
            Err(err) => {
                if let Some(message) =
                    auth_failure_message(codex_home, AccountOperation::TokenUsage, &err)
                {
                    auth_failure.get_or_insert(message);
                } else {
                    warnings.push(format!(
                        "Codex token usage summary could not be read: {err}"
                    ));
                }
                None
            }
        }
    } else {
        None
    };

    if let Some(message) = auth_failure {
        account_ok = false;
        account_message = message;
    }
    let auth_setup = (!account_ok).then(|| codex_auth_setup(codex_home, codex_binary));
    preconditions.push(ReadinessPrecondition::Account.view(account_ok, account_message.clone()));
    preconditions.push(ReadinessPrecondition::UsageLimits.view(
        reached.is_empty(),
        if reached.is_empty() {
            if rate_limits.is_empty() {
                "No active Codex rate-limit block was reported.".to_owned()
            } else {
                "Codex rate limits are available and no limit block is active.".to_owned()
            }
        } else {
            format!("Codex reports active limit block: {}.", reached.join(", "))
        },
    ));

    let usable = preconditions.iter().all(|precondition| precondition.ok);
    let payment_model = payment_model(account, plan_type.as_ref());
    let plan_type = plan_type.map(|plan_type| plan_type.as_str().to_owned());
    let message = if usable {
        match payment_model.as_deref() {
            Some(payment_model) => format!("Codex SDK is usable for automation ({payment_model})."),
            None => "Codex SDK is usable for automation.".to_owned(),
        }
    } else {
        let failed = preconditions
            .iter()
            .find(|precondition| !precondition.ok)
            .map(|precondition| precondition.message.clone())
            .unwrap_or_else(|| "A Codex automation precondition failed.".to_owned());
        format!("Codex SDK is unusable for automation. {failed}")
    };

    CodexAppServerStatusView {
        available: true,
        usable,
        message,
        install_prompt: CODEX_INSTALL_PROMPT.to_owned(),
        auth_setup,
        checked_at,
        binary_path: Some(binary_path),
        requires_openai_auth,
        signed_in,
        auth_method,
        account_label,
        plan_type,
        payment_model,
        preconditions,
        rate_limits,
        usage_summary,
        warnings,
        log_storage: CodexLogStorageStatusView::default(),
    }
}

pub(super) async fn spawn_initialized_app_server(
    codex_home: &Path,
    codex_binary: &Path,
) -> Result<ManagedCodexAppServer> {
    let app_server = spawn_managed_app_server(
        codex_binary,
        &codex_home.join(APP_SERVER_STATUS_LOG_FILE),
        codex_environment_for_home(codex_home)?,
    )
    .await?;
    let init = InitializeParams::new(ClientInfo::new(
        CLIENT_NAME,
        CLIENT_TITLE,
        env!("CARGO_PKG_VERSION"),
    ));
    app_server
        .client()
        .initialize(init)
        .await
        .context("Codex app-server rejected initialize")?;
    app_server
        .client()
        .initialized()
        .await
        .context("Codex app-server rejected initialized notification")?;
    Ok(app_server)
}

pub(super) fn unavailable_status(checked_at: String, message: String) -> CodexAppServerStatusView {
    CodexAppServerStatusView {
        available: false,
        usable: false,
        message: message.clone(),
        install_prompt: CODEX_INSTALL_PROMPT.to_owned(),
        auth_setup: None,
        checked_at,
        binary_path: None,
        requires_openai_auth: None,
        signed_in: false,
        auth_method: None,
        account_label: None,
        plan_type: None,
        payment_model: None,
        preconditions: vec![ReadinessPrecondition::AppServer.view(false, message)],
        rate_limits: Vec::new(),
        usage_summary: None,
        warnings: Vec::new(),
        log_storage: CodexLogStorageStatusView::default(),
    }
}

pub(super) fn with_log_storage(
    mut status: CodexAppServerStatusView,
    log_storage: CodexLogStorageStatusView,
) -> CodexAppServerStatusView {
    status.log_storage = log_storage;
    status
}

/// Builds the concise server-startup guidance for a non-usable Codex status.
pub fn operator_guidance(status: &CodexAppServerStatusView) -> Vec<String> {
    let mut lines = vec![status.message.clone()];
    if !status.available {
        lines.push(status.install_prompt.clone());
    }
    if let Some(setup) = &status.auth_setup {
        lines.push("Sign in with Dispatch's managed Codex home by running:".to_owned());
        lines.push(setup.login_command.clone());
        lines.push(setup.refresh_instruction.clone());
        lines.push(setup.api_key_instruction.clone());
    }
    lines
}

pub(super) fn auth_failure_message(
    codex_home: &Path,
    operation: AccountOperation,
    error: &ClientError,
) -> Option<String> {
    let text = match error {
        ClientError::Rpc { error } => {
            let data = error
                .data
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_default();
            format!("{} {data}", error.message)
        }
        _ => error.to_string(),
    };
    is_invalidated_auth_error(&text).then(|| {
        let operation = operation.description();
        format!(
            "Codex credentials in Dispatch's managed Codex home ({}) were rejected while {operation}. Log out from Dispatch, then sign in again with the managed CODEX_HOME.",
            codex_home.display(),
        )
    })
}

pub(super) fn is_invalidated_auth_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("401 unauthorized")
        || message.contains("refresh_token_invalidated")
        || message.contains("token_invalidated")
        || message.contains("session has ended")
        || message.contains("authentication token has been invalidated")
}

pub(super) async fn read_account_response(
    client: &CodexClient,
) -> std::result::Result<AccountReadResponse, ClientError> {
    let response = client
        .account_read(GetAccountParams {
            // The following rate-limit read is the remote readiness check and surfaces rejected
            // or invalidated credentials. Avoid forcing a separate token refresh here.
            refresh_token: Some(false),
            extra: Map::new(),
        })
        .await?;
    deserialize_opaque_response(response.extra)
}

pub(super) async fn read_rate_limits_response(
    client: &CodexClient,
) -> std::result::Result<RateLimitsReadResponse, ClientError> {
    let response = client.account_rate_limits_read().await?;
    deserialize_opaque_response(response.extra)
}

pub(super) async fn read_usage_response(
    client: &CodexClient,
) -> std::result::Result<UsageReadResponse, ClientError> {
    let response = client
        .send_raw_request(
            ACCOUNT_USAGE_READ_METHOD,
            Value::Null,
            Some(USAGE_REQUEST_TIMEOUT),
        )
        .await?;
    deserialize_response(response)
}

pub(super) fn deserialize_opaque_response<T>(
    fields: Map<String, Value>,
) -> std::result::Result<T, ClientError>
where
    T: DeserializeOwned,
{
    deserialize_response(Value::Object(fields))
}

pub(super) fn deserialize_response<T>(response: Value) -> std::result::Result<T, ClientError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(response).map_err(ClientError::Serialization)
}

pub(super) fn account_status_message(
    codex_home: &Path,
    account: Option<&CodexAccount>,
    account_label: Option<&str>,
    requires_openai_auth: Option<bool>,
) -> String {
    match account.map(|account| &account.auth_method) {
        Some(CodexAuthMethod::ChatGpt) => account_label.map_or_else(
            || "Signed in with ChatGPT.".to_owned(),
            |label| format!("Signed in with ChatGPT as {label}."),
        ),
        Some(CodexAuthMethod::ApiKey) => "Signed in with an API key.".to_owned(),
        Some(CodexAuthMethod::AmazonBedrock) => "Signed in with Amazon Bedrock.".to_owned(),
        Some(CodexAuthMethod::Other(method)) => format!("Signed in with {method}."),
        None if requires_openai_auth == Some(false) => {
            "The active Codex provider does not require OpenAI authentication.".to_owned()
        }
        None => format!(
            "No Codex account is signed in for Dispatch's managed Codex home ({}).",
            codex_home.display(),
        ),
    }
}

pub(super) fn payment_model(
    account: Option<&CodexAccount>,
    plan_type: Option<&CodexPlanType>,
) -> Option<String> {
    match account.map(|account| &account.auth_method) {
        Some(CodexAuthMethod::ApiKey) => Some("per token (API key)".to_owned()),
        Some(CodexAuthMethod::ChatGpt) => plan_type.map(|plan_type| {
            let plan = plan_type.as_str();
            if plan_type.is_usage_based() {
                format!("usage-based ChatGPT workspace ({plan})")
            } else if matches!(plan_type, CodexPlanType::Unknown) {
                "ChatGPT subscription (unknown plan)".to_owned()
            } else {
                format!("ChatGPT subscription ({plan})")
            }
        }),
        Some(CodexAuthMethod::AmazonBedrock) => Some("amazonBedrock".to_owned()),
        Some(CodexAuthMethod::Other(method)) => Some(method.clone()),
        None => plan_type.map(|plan_type| format!("plan {}", plan_type.as_str())),
    }
}

pub(super) fn format_unix_timestamp(timestamp: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(timestamp)
        .ok()
        .and_then(|time| time.format(&Rfc3339).ok())
}

/// Spawns a Dispatch-owned Codex app-server for an automation run.
///
/// The supplied environment is extended with the project-specific `CODEX_HOME` and
/// `CODEX_SQLITE_HOME`. App-server stderr is appended to `stderr_path` on every platform.
///
/// # Errors
///
/// Returns an error when the project Codex home is missing, the app-server process cannot be
/// started, or its WebSocket endpoint does not become reachable.
pub async fn spawn_codex_with_home_and_env(
    codex_binary: &Path,
    codex_home: &Path,
    stderr_path: &Path,
    mut env: HashMap<String, String>,
) -> Result<ManagedCodexAppServer> {
    env.extend(codex_environment_for_home(codex_home)?);
    spawn_managed_app_server(codex_binary, stderr_path, env).await
}

pub(super) async fn spawn_managed_app_server(
    codex_binary: &Path,
    stderr_path: &Path,
    env: HashMap<String, String>,
) -> Result<ManagedCodexAppServer> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .context("failed to reserve a loopback port for Codex app-server")?;
    let address = listener
        .local_addr()
        .context("failed to read the reserved Codex app-server address")?;
    let url = format!("ws://{address}");
    drop(listener);

    let mut command = Command::new(codex_binary);
    command.arg("app-server").arg("--listen").arg(&url);
    apply_app_server_environment(&mut command, env);
    let process = spawn_app_server_process(command, stderr_path).await?;
    connect_managed_app_server(process, url).await
}

pub(super) fn apply_app_server_environment(command: &mut Command, env: HashMap<String, String>) {
    // The server may itself be running inside a Dispatch-launched process. Reserve the complete
    // namespace so stale database, project, run, API, Git-policy, and server configuration cannot
    // cross into a new app-server; the explicit overlay below is the only Dispatch context passed.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("DISPATCH_") {
            command.env_remove(key);
        }
    }
    command.envs(env);
}

pub(super) async fn connect_managed_app_server(
    process: AppServerProcess,
    url: String,
) -> Result<ManagedCodexAppServer> {
    connect_managed_app_server_with_timeout(process, url, APP_SERVER_CONNECT_TIMEOUT).await
}

pub(super) async fn connect_managed_app_server_with_timeout(
    process: AppServerProcess,
    url: String,
    connect_timeout: Duration,
) -> Result<ManagedCodexAppServer> {
    let deadline = Instant::now() + connect_timeout;
    loop {
        if process.is_finished() {
            process
                .join_monitor()
                .await
                .context("Codex app-server exited before its WebSocket endpoint became ready")?;
            bail!("Codex app-server exited before its WebSocket endpoint became ready");
        }

        let connect = CodexClient::connect_ws(WsConfig::new(
            url.clone(),
            HashMap::new(),
            Default::default(),
        ));
        let last_error =
            match timeout(deadline.saturating_duration_since(Instant::now()), connect).await {
                Ok(Ok(client)) => return Ok(ManagedCodexAppServer { client, process }),
                Ok(Err(err)) => err.to_string(),
                Err(_) => "timed out during the WebSocket handshake".to_owned(),
            };

        if Instant::now() >= deadline {
            let cleanup_error = process.shutdown().await.err();
            if let Some(cleanup_error) = cleanup_error {
                bail!(
                    "Codex app-server WebSocket endpoint {url} did not become ready: {last_error}. Process cleanup also failed: {cleanup_error:#}"
                );
            }
            bail!(
                "Codex app-server WebSocket endpoint {url} did not become ready within {connect_timeout:?}: {last_error}"
            );
        }

        sleep(APP_SERVER_CONNECT_RETRY_INTERVAL).await;
    }
}

pub(super) async fn spawn_app_server_process(
    command: Command,
    stderr_path: &Path,
) -> Result<AppServerProcess> {
    if let Some(parent) = stderr_path.parent() {
        fs::create_dir_all(parent).context_with(|| {
            format!(
                "failed to create Codex app-server log directory {}",
                parent.display()
            )
        })?;
    }
    let stderr_file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(stderr_path)
        .await
        .context_with(|| {
            format!(
                "failed to open Codex app-server log {}",
                stderr_path.display()
            )
        })?;

    let process = Process::new(command)
        .name("Codex app-server")
        .stdout(ProcessStreamBuilder::discard)
        .stderr(|stream| {
            stream
                .single_subscriber()
                .reliable_with_backpressure()
                .replay_all()
                .read_chunk_size(DEFAULT_READ_CHUNK_SIZE)
                .max_buffered_chunks(DEFAULT_MAX_BUFFERED_CHUNKS)
        })
        .spawn()
        .context("failed to start Codex app-server process")?;
    let pid = process.id();
    let stderr_consumer = process
        .stderr()
        .consume_async(WriteChunks::passthrough(
            "stderr",
            stderr_file,
            WriteCollectionOptions::log_and_continue(),
        ))
        .context("failed to attach the Codex app-server stderr collector")?;
    process.stderr().seal_replay();

    let mut process = process.terminate_on_drop(app_server_shutdown_policy());
    let (shutdown, mut shutdown_requested) = oneshot::channel();
    let monitor = tokio::spawn(async move {
        let shutdown_was_requested = loop {
            let event = tokio::select! {
                biased;
                _ = &mut shutdown_requested => Some(true),
                result = process.wait_for_completion(APP_SERVER_EXIT_POLL_INTERVAL) => {
                    match result.context("failed while waiting for Codex app-server")? {
                        WaitForCompletionResult::Completed(_) => Some(false),
                        WaitForCompletionResult::Timeout { .. } => None,
                    }
                }
            };
            if let Some(shutdown_was_requested) = event {
                break shutdown_was_requested;
            }
        };

        if shutdown_was_requested {
            process
                .terminate(app_server_shutdown_policy())
                .await
                .context("failed to terminate Codex app-server")?;
        }

        timeout(APP_SERVER_LOG_DRAIN_TIMEOUT, stderr_consumer.wait())
            .await
            .context("timed out while draining Codex app-server stderr")?
            .context("failed to join the Codex app-server stderr collector")?
            .context("failed to write Codex app-server stderr")?;

        if !shutdown_was_requested {
            tracing::warn!("Codex app-server exited before Dispatch requested shutdown");
        }
        Ok(())
    });

    Ok(AppServerProcess {
        pid,
        shutdown: Some(shutdown),
        monitor: Some(monitor),
    })
}

pub(super) fn app_server_shutdown_policy() -> GracefulShutdown {
    GracefulShutdown::builder()
        .unix_sigterm(Duration::from_secs(2))
        .windows_ctrl_break(Duration::from_secs(2))
        .build()
}

/// Prepared knowledge jobs retain their own runtime configuration and reuse shared authentication.
pub(crate) fn ensure_isolated_codex_home(
    codex_home: &Path,
    settings: &ProjectSettingsView,
    project_home: &Path,
) -> Result<PathBuf> {
    ensure_codex_home_at(codex_home)?;
    let shared_home = codex_home;
    let project_home = project_home.to_path_buf();
    fs::create_dir_all(&project_home).context_with(|| {
        format!(
            "failed to create Dispatch project Codex home {}",
            project_home.display()
        )
    })?;
    for entry in [
        SharedCodexEntry::Auth,
        SharedCodexEntry::InstallationId,
        SharedCodexEntry::Skills,
    ] {
        link_shared_codex_entry(shared_home, &project_home, entry)?;
    }
    write_project_codex_config(&project_home, settings)?;
    write_project_git_rules(&project_home, settings)?;
    Ok(project_home)
}

pub(super) fn codex_auth_setup(codex_home: &Path, codex_binary: &Path) -> CodexAuthSetupView {
    let codex_config = codex_config_path_for_home(codex_home);
    CodexAuthSetupView {
        codex_home_path: codex_home.to_string_lossy().into_owned(),
        codex_config_path: codex_config.to_string_lossy().into_owned(),
        login_command: codex_login_command_for(codex_binary, codex_home),
        refresh_instruction:
            "After the browser login completes, return to Dispatch and use Refresh to check the new account state.".to_owned(),
        api_key_instruction:
            "For API-key auth instead, start the Dispatch server with OPENAI_API_KEY set."
                .to_owned(),
    }
}

pub(super) fn codex_login_command_for(codex_binary: &Path, codex_home: &Path) -> String {
    let codex_home = codex_home.to_string_lossy();
    let codex_binary = codex_binary.to_string_lossy();
    format!(
        "CODEX_HOME={} CODEX_SQLITE_HOME={} {} login",
        shell_quote(codex_home.as_ref()),
        shell_quote(codex_home.as_ref()),
        shell_quote(codex_binary.as_ref()),
    )
}

pub(super) fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        return value.to_owned();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn codex_environment_for_home(codex_home: &Path) -> Result<HashMap<String, String>> {
    if !codex_home.is_dir() {
        bail!(
            "Dispatch Codex home {} does not exist or is not a directory",
            codex_home.display()
        );
    }
    let codex_home = utf8_path(codex_home, "Codex home")?;
    Ok(HashMap::from([
        ("CODEX_HOME".to_owned(), codex_home.clone()),
        ("CODEX_SQLITE_HOME".to_owned(), codex_home),
    ]))
}

pub(crate) fn codex_home_dir_for_dispatch_home(dispatch_home: &Path) -> PathBuf {
    dispatch_home.join(CODEX_HOME_DIR)
}

pub(super) fn codex_config_path_for_home(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

pub(super) fn ensure_codex_home_at(codex_home: &Path) -> Result<()> {
    fs::create_dir_all(codex_home).context_with(|| {
        format!(
            "failed to create Dispatch Codex home {}",
            codex_home.display()
        )
    })?;
    let config_path = codex_config_path_for_home(codex_home);
    fs::write(&config_path, CODEX_CONFIG)
        .context_with(|| format!("failed to write Codex config {}", config_path.display()))?;
    Ok(())
}

pub(super) fn link_shared_codex_entry(
    shared_home: &Path,
    project_home: &Path,
    entry: SharedCodexEntry,
) -> Result<()> {
    let entry_name = entry.file_name();
    let source = shared_home.join(entry_name);
    let destination = project_home.join(entry_name);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let current_target = fs::read_link(&destination).context_with(|| {
                format!(
                    "failed to read project Codex link {}",
                    destination.display()
                )
            })?;
            let current_target = if current_target.is_absolute() {
                current_target
            } else {
                destination
                    .parent()
                    .unwrap_or(project_home)
                    .join(current_target)
            };
            if source.exists() && current_target == source {
                return Ok(());
            }
            fs::remove_file(&destination).context_with(|| {
                format!(
                    "failed to remove stale project Codex link {}",
                    destination.display()
                )
            })?;
        }
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .context_with(|| format!("failed to inspect {}", destination.display()))?;
        }
    }
    if !source.exists() {
        return Ok(());
    }
    symlink_path(&source, &destination).context_with(|| {
        format!(
            "failed to link shared Codex entry {} into {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn symlink_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, destination)
}

#[cfg(windows)]
pub(super) fn symlink_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, destination)
    } else {
        std::os::windows::fs::symlink_file(source, destination)
    }
}

pub(super) fn write_project_codex_config(
    codex_home: &Path,
    settings: &ProjectSettingsView,
) -> Result<()> {
    let config_path = codex_config_path_for_home(codex_home);
    let sandbox_mode = match settings.agent_sandbox_mode {
        AgentSandboxMode::WorkspaceWrite => "workspace-write",
        AgentSandboxMode::DangerFullAccess => "danger-full-access",
    };
    let writable_roots = toml_string_array(&settings.agent_extra_writable_roots);
    let config = format!(
        r#"# Managed by Dispatch.
# This file is regenerated from the Dispatch project settings.

check_for_update_on_startup = false
approval_policy = "never"
sandbox_mode = {sandbox_mode}

[features]
apps = false
memories = false
remote_plugin = false

[memories]
use_memories = false
generate_memories = false
disable_on_external_context = true

[sandbox_workspace_write]
network_access = true
writable_roots = {writable_roots}
"#,
        sandbox_mode = toml_string(sandbox_mode),
    );
    fs::write(&config_path, config)
        .context_with(|| format!("failed to write Codex config {}", config_path.display()))?;
    Ok(())
}

pub(super) fn write_project_git_rules(
    codex_home: &Path,
    settings: &ProjectSettingsView,
) -> Result<()> {
    let rules_dir = codex_home.join("rules");
    fs::create_dir_all(&rules_dir)
        .context_with(|| format!("failed to create Codex rules dir {}", rules_dir.display()))?;
    let rules_path = rules_dir.join(PROJECT_RULES_FILE_NAME);
    fs::write(&rules_path, git_rules_for_policy(settings)).context_with(|| {
        format!(
            "failed to write Codex git rules file {}",
            rules_path.display()
        )
    })?;
    Ok(())
}

pub(super) fn git_rules_for_policy(settings: &ProjectSettingsView) -> String {
    let policy = &settings.agent_git_command_policy;
    let mut rules = String::from(
        r#"# Managed by Dispatch.
# These rules mirror this project's Dispatch Git command policy. Dispatch also
# puts a guarded git shim first on PATH for checks that command prefixes cannot express.

"#,
    );
    if policy.add {
        rules.push_str(&prefix_rule(
            &["git", "add"],
            RuleDecision::Allow,
            "Dispatch allows staging explicit changes for this project.",
        ));
    }
    if policy.commit {
        rules.push_str(&prefix_rule(
            &["git", "commit"],
            RuleDecision::Allow,
            "Dispatch allows commits for this project; the git wrapper enforces --no-verify.",
        ));
    }
    if policy.push {
        rules.push_str(&prefix_rule(
            &["git", "push"],
            RuleDecision::Allow,
            "Dispatch allows non-force pushes for this project.",
        ));
        for forbidden in [
            ["git", "push", "--force"],
            ["git", "push", "-f"],
            ["git", "push", "--force-with-lease"],
            ["git", "push", "--mirror"],
            ["git", "push", "--delete"],
            ["git", "push", "--prune"],
        ] {
            rules.push_str(&prefix_rule(
                &forbidden,
                RuleDecision::Forbidden,
                "Dispatch blocks force, mirror, delete, and prune pushes; use a normal git push.",
            ));
        }
    }
    if policy.reset {
        rules.push_str(&prefix_rule(
            &["git", "reset"],
            RuleDecision::Allow,
            "Dispatch allows git reset within this project's configured limits.",
        ));
        if !policy.allows_hard_reset(settings.workspace_mode) {
            rules.push_str(&prefix_rule(
                &["git", "reset", "--hard"],
                RuleDecision::Forbidden,
                "Dispatch blocks git reset --hard for the current workspace mode.",
            ));
        }
    }
    rules
}

pub(super) fn prefix_rule(pattern: &[&str], decision: RuleDecision, justification: &str) -> String {
    let pattern = pattern
        .iter()
        .map(|value| toml_string(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"prefix_rule(
    pattern = [{pattern}],
    decision = {decision},
    justification = {justification},
)

"#,
        decision = toml_string(decision.as_str()),
        justification = toml_string(justification),
    )
}

pub(super) fn toml_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| toml_string(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{values}]")
}

pub(super) fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("TOML-compatible string must serialize")
}

pub(super) fn utf8_path(path: &Path, description: &str) -> Result<String> {
    let Some(path) = path.to_str() else {
        bail!("{description} is not valid UTF-8 and cannot be passed to the Codex SDK");
    };
    Ok(path.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::backend::{
        execution::codex::logs as codex_log_storage, execution::sessions::ProcessSessionRegistry,
    };
    use crate::shared::view_models::AgentToolName;
    use std::sync::Arc;

    use assertr::prelude::*;
    use std::{
        fs::OpenOptions,
        path::{Path, PathBuf},
    };

    use tempfile::TempDir;

    use super::super::service::CodexService;
    use super::*;
    use crate::backend::{execution::sessions::ProcessSessionStart, storage::Store};
    use crate::shared::view_models::{
        AgentGitCommandPolicy, AgentReasoningEffort, CodexLogDatabaseView, RevertStrategy,
        WorkspaceMode, WorktreeCleanupPolicy,
    };
    use tokio::sync::RwLock;

    fn sparse_file(path: &Path, size_bytes: u64) {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        file.set_len(size_bytes).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn app_server_subprocess_environment_sanitizes_reserved_dispatch_context() {
        const CHILD_MARKER: &str = "APP_SERVER_ENV_SANITIZATION_CHILD";
        const ORDINARY_PARENT_KEY: &str = "APP_SERVER_ENV_SANITIZATION_ORDINARY";
        const STALE_DISPATCH_KEYS: &[&str] = &[
            "DISPATCH_DATABASE",
            "DISPATCH_PROJECT",
            "DISPATCH_AGENT_ID",
            "DISPATCH_AGENT_RUN_ID",
            "DISPATCH_CLAIMED_ITEM_ID",
            "DISPATCH_API_URL",
            "DISPATCH_URL",
            "DISPATCH_GIT_POLICY_PATH",
            "DISPATCH_REAL_GIT",
            "DISPATCH_DEVELOPMENT",
            "DISPATCH_BIND",
            "DISPATCH_LOG",
            "DISPATCH_SQLX_LOG",
            "DISPATCH_CLI_TARGET_DIR",
            "DISPATCH_FUTURE_RESERVED_CONTEXT",
        ];

        if std::env::var_os(CHILD_MARKER).is_none() {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .arg("app_server_subprocess_environment_sanitizes_reserved_dispatch_context")
                .arg("--nocapture")
                .env(CHILD_MARKER, "1")
                .env(ORDINARY_PARENT_KEY, "preserved-parent-value");
            for key in STALE_DISPATCH_KEYS {
                child.env(key, format!("stale-{key}"));
            }
            let output = child.output().await.unwrap();
            if !output.status.success() {
                eprintln!(
                    "child test stdout:\n{}",
                    String::from_utf8_lossy(&output.stdout)
                );
                eprintln!(
                    "child test stderr:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            assert_that!(&(output.status.success())).is_true();
            assert_that!(&(String::from_utf8_lossy(&output.stdout).contains("1 passed"))).is_true();
            return;
        }

        // This is the production run-overlay builder, and the helper below is called immediately
        // before the shared app-server spawn. Executing `/usr/bin/env` keeps the inheritance check
        // independent from the Codex WebSocket handshake while exercising that exact boundary.
        let git_runtime = crate::backend::execution::git::GitRuntimeFiles {
            shim_dir: PathBuf::from("/tmp/run-41-bin"),
            policy_path: PathBuf::from("/tmp/run-41.git-policy.json"),
        };
        let expected_dispatch_context =
            crate::backend::execution::git::GitRuntime::new(std::env::var_os("PATH"))
                .agent_environment(
                    Path::new("/tmp/dispatch"),
                    &git_runtime,
                    Path::new("/usr/bin/git"),
                    "demo",
                    "dispatch-run-41",
                    Some(73),
                    Some("http://127.0.0.1:4000"),
                );
        let mut subprocess = Command::new("/usr/bin/env");
        apply_app_server_environment(&mut subprocess, expected_dispatch_context.clone());

        let output = subprocess.output().await.unwrap();
        let actual_environment = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect::<HashMap<_, _>>();

        assert_that!(&(output.status.success())).is_true();
        assert_that!(
            &(actual_environment
                .get(ORDINARY_PARENT_KEY)
                .map(String::as_str))
        )
        .is_equal_to(Some("preserved-parent-value"));
        for key in STALE_DISPATCH_KEYS {
            assert_that!(&(actual_environment.get(*key)))
                .is_equal_to(expected_dispatch_context.get(*key));
        }
    }

    #[tokio::test]
    async fn unavailable_app_server_status_prompts_for_codex_install() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        crate::backend::execution::tools::tests::service(&store)
            .set_path(
                AgentToolName::Codex,
                PathBuf::from("/definitely/missing/codex"),
            )
            .await
            .unwrap();

        let status = service_with_store(
            &store,
            temp.path(),
            ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new()),
        )
        .status()
        .await;

        assert_that!(&(!status.available)).is_true();
        assert_that!(&(status.message.contains("Codex app-server is unavailable"))).is_true();
        assert_that!(&(status.install_prompt.contains("Install Codex"))).is_true();
    }

    #[test]
    fn storage_findings_survive_app_server_failure_without_changing_readiness() {
        let log_storage = CodexLogStorageStatusView {
            threshold_bytes: codex_log_storage::CODEX_LOG_DATABASE_THRESHOLD_BYTES,
            oversized_databases: vec![CodexLogDatabaseView {
                relative_path: "projects/42/logs_1.sqlite".to_owned(),
                size_bytes: codex_log_storage::CODEX_LOG_DATABASE_THRESHOLD_BYTES + 1,
            }],
            scan_errors: Vec::new(),
        };

        let status = with_log_storage(
            unavailable_status(
                "now".to_owned(),
                "Codex app-server initialization failed".to_owned(),
            ),
            log_storage.clone(),
        );

        assert_that!(&(!status.available)).is_true();
        assert_that!(&(!status.usable)).is_true();
        assert_that!(&(status.log_storage)).is_equal_to(log_storage);
    }

    #[tokio::test]
    async fn purge_refuses_an_active_codex_session_without_removing_logs() {
        let home = TempDir::new().unwrap();
        let log = home.path().join("logs_active.sqlite");
        sparse_file(
            &log,
            codex_log_storage::CODEX_LOG_DATABASE_THRESHOLD_BYTES + 1,
        );
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: AgentToolName::Codex.as_storage().to_owned(),
            command: String::new(),
            working_dir: String::new(),
        });

        let result = service(home.path(), sessions.clone())
            .await
            .purge_logs_inner()
            .await;

        assert_that!(&(result.is_err())).is_true();
        assert_that!(&(format!("{:#}", result.unwrap_err()))).contains("1 Codex session is active");
        assert_that!(&(log.exists())).is_true();
    }

    #[tokio::test]
    async fn purge_waits_for_probe_lock_and_holds_codex_session_admission() {
        let home = TempDir::new().unwrap();
        let log = home.path().join("logs_waiting.sqlite");
        sparse_file(
            &log,
            codex_log_storage::CODEX_LOG_DATABASE_THRESHOLD_BYTES + 1,
        );
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let codex = Arc::new(service(home.path(), sessions.clone()).await);
        let operation = codex.operation.lock().await;
        let cleanup_codex = codex.clone();
        let cleanup = tokio::spawn(async move { cleanup_codex.purge_logs_inner().await });
        for _ in 0..100 {
            if sessions.codex_maintenance_active() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_that!(&(sessions.codex_maintenance_active())).is_true();
        let concurrent = sessions.begin(ProcessSessionStart {
            run_id: 8,
            project_id: 1,
            project_name: "demo".to_owned(),
            tool_name: AgentToolName::Codex.as_storage().to_owned(),
            command: String::new(),
            working_dir: String::new(),
        });
        assert_that!(&(!concurrent.is_registered())).is_true();
        assert_that!(&(log.exists())).is_true();

        drop(operation);
        let result = cleanup.await.unwrap().unwrap();

        assert_that!(&(result.removed_database_count)).is_equal_to(1);
        assert_that!(&(!log.exists())).is_true();
        assert_that!(&(!sessions.codex_maintenance_active())).is_true();
    }

    #[test]
    fn codex_login_command_uses_managed_home_and_binary() {
        let command = codex_login_command_for(
            Path::new("/opt/codex/bin/codex"),
            Path::new("/Users/test/.dispatch/codex"),
        );

        assert_that!(&(command)).is_equal_to("CODEX_HOME=/Users/test/.dispatch/codex CODEX_SQLITE_HOME=/Users/test/.dispatch/codex /opt/codex/bin/codex login");
    }

    #[test]
    fn codex_login_command_quotes_shell_values() {
        let command = codex_login_command_for(
            Path::new("/Applications/Codex CLI/codex"),
            Path::new("/Users/test/Dispatch Home/codex"),
        );

        assert_that!(&(command)).is_equal_to("CODEX_HOME='/Users/test/Dispatch Home/codex' CODEX_SQLITE_HOME='/Users/test/Dispatch Home/codex' '/Applications/Codex CLI/codex' login");
    }

    #[test]
    fn operator_guidance_for_auth_setup_omits_install_prompt() {
        let status = CodexAppServerStatusView {
            available: true,
            usable: false,
            message: "Codex SDK is unusable for automation.".to_owned(),
            install_prompt: CODEX_INSTALL_PROMPT.to_owned(),
            auth_setup: Some(CodexAuthSetupView {
                codex_home_path: "/Users/test/.dispatch/codex".to_owned(),
                codex_config_path: "/Users/test/.dispatch/codex/config.toml".to_owned(),
                login_command: "CODEX_HOME=/Users/test/.dispatch/codex codex login".to_owned(),
                refresh_instruction: "Refresh after login.".to_owned(),
                api_key_instruction: "Set OPENAI_API_KEY.".to_owned(),
            }),
            ..Default::default()
        };

        let guidance = operator_guidance(&status).join("\n");

        assert_that!(&(guidance.contains("CODEX_HOME=/Users/test/.dispatch/codex codex login")))
            .is_true();
        assert_that!(&(!guidance.contains(CODEX_INSTALL_PROMPT))).is_true();
    }

    #[test]
    fn invalidated_auth_errors_are_account_failures() {
        let error = ClientError::Rpc {
            error: codex_app_server_sdk::RpcError {
                code: -32603,
                message: "failed to fetch codex rate limits: 401 Unauthorized token_invalidated"
                    .to_owned(),
                data: None,
            },
        };

        let message = auth_failure_message(
            Path::new("/managed/codex"),
            AccountOperation::RateLimits,
            &error,
        )
        .unwrap();

        assert_that!(&(message.contains("managed Codex home"))).is_true();
        assert_that!(&(message.contains("Log out"))).is_true();
    }

    #[test]
    fn account_response_is_decoded_into_protocol_types() {
        let response = deserialize_response::<AccountReadResponse>(serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "operator@example.com",
                "planType": "pro"
            },
            "requiresOpenaiAuth": true
        }))
        .unwrap();
        let account = response.account.as_ref().unwrap();

        assert_that!(&(response.requires_openai_auth)).is_equal_to(Some(true));
        assert_that!(&(account.auth_method())).is_equal_to("chatgpt");
        assert_that!(&(account.label().as_deref())).is_equal_to(Some("operator@example.com"));
        assert_that!(&(account.plan_type().as_ref().map(CodexPlanType::as_str)))
            .is_equal_to(Some("pro"));
    }

    #[test]
    fn rate_limits_prefer_typed_multi_bucket_response() {
        let response = deserialize_response::<RateLimitsReadResponse>(serde_json::json!({
            "rateLimits": {
                "limitName": "legacy",
                "primary": { "usedPercent": 5 }
            },
            "rateLimitsByLimitId": {
                "codex": {
                    "planType": "plus",
                    "primary": {
                        "usedPercent": 42,
                        "windowDurationMins": 300
                    },
                    "rateLimitReachedType": "rate_limit_reached"
                }
            }
        }))
        .unwrap();

        assert_that!(&(response.plan_type().as_ref().map(CodexPlanType::as_str)))
            .is_equal_to(Some("plus"));
        let limits = response.into_views();
        assert_that!(&(limits.len())).is_equal_to(1);
        assert_that!(&(limits[0].label)).is_equal_to("codex");
        assert_that!(&(limits[0].primary_used_percent)).is_equal_to(Some(42));
        assert_that!(&(limits[0].primary_window_minutes)).is_equal_to(Some(300));
        assert_that!(&(limits[0].reached_type.as_deref())).is_equal_to(Some("rate_limit_reached"));
    }

    #[test]
    fn malformed_rate_limit_numbers_fail_at_the_protocol_boundary() {
        let result = deserialize_response::<RateLimitsReadResponse>(serde_json::json!({
            "rateLimits": {
                "primary": { "usedPercent": "forty-two" }
            }
        }));

        assert_that!(&(matches!(result, Err(ClientError::Serialization(_))))).is_true();
    }

    #[test]
    fn protocol_enum_extensions_remain_visible() {
        let account = deserialize_response::<AccountReadResponse>(serde_json::json!({
            "account": { "type": "futureProvider" },
            "requiresOpenaiAuth": false
        }))
        .unwrap()
        .account
        .unwrap();

        assert_that!(&(account.auth_method())).is_equal_to("futureProvider");
        assert_that!(&(account.label().as_deref())).is_equal_to(Some("futureProvider"));

        let response = deserialize_response::<RateLimitsReadResponse>(serde_json::json!({
            "rateLimits": {
                "planType": "future_plan",
                "rateLimitReachedType": "future_limit"
            }
        }))
        .unwrap();
        let limits = response.into_views();

        assert_that!(&(limits[0].plan_type.as_deref())).is_equal_to(Some("future_plan"));
        assert_that!(&(limits[0].reached_type.as_deref())).is_equal_to(Some("future_limit"));
    }

    #[test]
    fn rate_limit_percentages_use_the_protocol_integer_width() {
        let wide_percentage = i64::from(i32::MAX) + 1;
        let response = deserialize_response::<RateLimitsReadResponse>(serde_json::json!({
            "rateLimits": {
                "primary": { "usedPercent": wide_percentage },
                "individualLimit": { "remainingPercent": wide_percentage }
            }
        }))
        .unwrap();
        let limits = response.into_views();

        assert_that!(&(limits[0].primary_used_percent)).is_equal_to(Some(wide_percentage));
        assert_that!(&(limits[0].individual_remaining_percent)).is_equal_to(Some(wide_percentage));
    }

    #[test]
    fn usage_response_is_decoded_into_status_view() {
        let response = deserialize_response::<UsageReadResponse>(serde_json::json!({
            "summary": {
                "lifetimeTokens": 1200,
                "peakDailyTokens": 300,
                "currentStreakDays": 4,
                "longestStreakDays": 9,
                "longestRunningTurnSec": 75
            },
            "dailyUsageBuckets": []
        }))
        .unwrap();
        let summary = CodexUsageSummaryView::from(response.summary.unwrap());

        assert_that!(&(summary.lifetime_tokens)).is_equal_to(Some(1200));
        assert_that!(&(summary.peak_daily_tokens)).is_equal_to(Some(300));
        assert_that!(&(summary.current_streak_days)).is_equal_to(Some(4));
        assert_that!(&(summary.longest_streak_days)).is_equal_to(Some(9));
        assert_that!(&(summary.longest_running_turn_seconds)).is_equal_to(Some(75));
    }

    #[test]
    fn codex_home_config_disables_memories() {
        let temp = TempDir::new().unwrap();
        let codex_home = codex_home_dir_for_dispatch_home(temp.path());

        ensure_codex_home_at(&codex_home).unwrap();

        let config = fs::read_to_string(codex_config_path_for_home(&codex_home)).unwrap();
        assert_that!(&(config.contains("[features]"))).is_true();
        assert_that!(&(config.contains("check_for_update_on_startup = false"))).is_true();
        assert_that!(&(config.contains("apps = false"))).is_true();
        assert_that!(&(config.contains("memories = false"))).is_true();
        assert_that!(&(config.contains("remote_plugin = false"))).is_true();
        assert_that!(&(config.contains("[memories]"))).is_true();
        assert_that!(&(config.contains("use_memories = false"))).is_true();
        assert_that!(&(config.contains("generate_memories = false"))).is_true();
    }

    #[test]
    fn project_codex_config_uses_project_sandbox_settings() {
        let temp = TempDir::new().unwrap();
        let mut settings = project_settings();
        settings.agent_extra_writable_roots = vec!["/tmp/dispatch-browser".to_owned()];

        write_project_codex_config(temp.path(), &settings).unwrap();

        let config = fs::read_to_string(codex_config_path_for_home(temp.path())).unwrap();
        assert_that!(&(config.contains("approval_policy = \"never\""))).is_true();
        assert_that!(&(config.contains("sandbox_mode = \"workspace-write\""))).is_true();
        assert_that!(&(config.contains("network_access = true"))).is_true();
        assert_that!(&(config.contains("writable_roots = [\"/tmp/dispatch-browser\"]"))).is_true();
        assert_that!(&(config.contains("check_for_update_on_startup = false"))).is_true();
        assert_that!(&(config.contains("apps = false"))).is_true();
        assert_that!(&(config.contains("memories = false"))).is_true();
        assert_that!(&(config.contains("remote_plugin = false"))).is_true();
    }

    #[test]
    fn readiness_probe_omits_optional_usage_request() {
        assert_that!(&(!StatusProbe::Readiness.includes_usage())).is_true();
        assert_that!(&(StatusProbe::Detailed.includes_usage())).is_true();
    }

    #[tokio::test]
    async fn detailed_status_refresh_reuses_recent_detailed_result() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        let refresh = service_with_store(
            &store,
            temp.path(),
            ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new()),
        );
        let shared = RwLock::new(CodexAppServerStatusView::default());
        let expected = CodexAppServerStatusView {
            available: true,
            usable: true,
            checked_at: "recent-detailed-status".to_owned(),
            ..Default::default()
        };
        refresh.store_detailed(&shared, expected).await;
        *shared.write().await = CodexAppServerStatusView {
            checked_at: "newer-readiness-snapshot".to_owned(),
            ..Default::default()
        };

        let cached = refresh
            .refresh_if_stale(&shared, Duration::from_secs(60))
            .await;

        assert_that!(&(cached.checked_at)).is_equal_to("recent-detailed-status");
        assert_that!(&(cached.available)).is_true();
        assert_that!(&(cached.usable)).is_true();
    }

    #[tokio::test]
    async fn readiness_snapshot_replaces_shared_status() {
        let home = TempDir::new().unwrap();
        let codex = service(
            home.path(),
            ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new()),
        )
        .await;
        let shared = Arc::new(RwLock::new(CodexAppServerStatusView::default()));
        let readiness = CodexAppServerStatusView {
            available: true,
            usable: true,
            checked_at: "pre-run-readiness".to_owned(),
            ..Default::default()
        };

        codex.publish_snapshot(&shared, readiness).await;

        let shared = shared.read().await;
        assert_that!(&(shared.checked_at)).is_equal_to("pre-run-readiness");
        assert_that!(&(shared.available)).is_true();
        assert_that!(&(shared.usable)).is_true();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_handshake_cannot_outlive_connection_deadline() {
        let temp = TempDir::new().unwrap();
        let stderr_path = temp.path().join("app-server.stderr.log");
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let stalled_server = tokio::spawn(async move {
            let (_connection, _) = listener.accept().await.unwrap();
            sleep(Duration::from_secs(30)).await;
        });
        let mut command = Command::new("sleep");
        command.arg("30");
        let process = spawn_app_server_process(command, &stderr_path)
            .await
            .unwrap();

        let started_at = Instant::now();
        let error =
            match connect_managed_app_server_with_timeout(process, url, Duration::from_millis(100))
                .await
            {
                Ok(_) => panic!("stalled WebSocket handshake should time out"),
                Err(error) => error,
            };
        stalled_server.abort();

        assert_that!(
            &(error
                .to_string()
                .contains("timed out during the WebSocket handshake"))
        )
        .is_true();
        assert_that!(&(started_at.elapsed() < Duration::from_secs(3))).is_true();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn managed_app_server_shutdown_terminates_reaps_and_drains_stderr() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let fake_codex = temp.path().join("codex");
        let ready = temp.path().join("ready");
        let exited = temp.path().join("exited");
        let stderr_path = temp.path().join("app-server.stderr.log");
        fs::write(
            &fake_codex,
            r#"#!/bin/sh
marker="$(dirname "$0")/exited"
trap 'touch "$marker"; exit 0' TERM INT
printf '%s\n' 'fake app-server started' >&2
touch "$(dirname "$0")/ready"
while :; do sleep 1; done
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_codex, permissions).unwrap();

        let process = spawn_app_server_process(Command::new(&fake_codex), &stderr_path)
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fake app-server did not become ready");
        process.shutdown().await.unwrap();

        assert_that!(&(exited.exists())).is_true();
        assert_that!(
            &(fs::read_to_string(stderr_path)
                .unwrap()
                .contains("fake app-server started"))
        )
        .is_true();
    }

    #[cfg(unix)]
    #[test]
    fn project_codex_home_repairs_stale_shared_links() {
        let temp = TempDir::new().unwrap();
        let shared_home = temp.path().join("dispatch/codex");
        let project_home = shared_home.join("projects/7");
        let stale_home = temp.path().join("patchbay/codex");
        fs::create_dir_all(&project_home).unwrap();
        fs::create_dir_all(&shared_home).unwrap();
        fs::write(shared_home.join("installation_id"), "current-installation").unwrap();
        std::os::unix::fs::symlink(
            stale_home.join("installation_id"),
            project_home.join("installation_id"),
        )
        .unwrap();

        link_shared_codex_entry(
            &shared_home,
            &project_home,
            SharedCodexEntry::InstallationId,
        )
        .unwrap();

        assert_that!(&(fs::read_link(project_home.join("installation_id")).unwrap()))
            .is_equal_to(shared_home.join("installation_id"));
    }

    #[cfg(unix)]
    #[test]
    fn project_codex_home_removes_stale_link_when_shared_entry_is_absent() {
        let temp = TempDir::new().unwrap();
        let shared_home = temp.path().join("dispatch/codex");
        let project_home = shared_home.join("projects/7");
        fs::create_dir_all(&project_home).unwrap();
        std::os::unix::fs::symlink(
            temp.path().join("patchbay/codex/auth.json"),
            project_home.join("auth.json"),
        )
        .unwrap();

        link_shared_codex_entry(&shared_home, &project_home, SharedCodexEntry::Auth).unwrap();

        assert_that!(&(fs::symlink_metadata(project_home.join("auth.json")).is_err())).is_true();
    }

    #[test]
    fn project_git_rules_reflect_allowed_commands_and_hard_reset() {
        let mut settings = project_settings();
        settings.workspace_mode = WorkspaceMode::CurrentBranch;
        settings.agent_git_command_policy = AgentGitCommandPolicy {
            add: true,
            commit: true,
            push: true,
            reset: true,
            ..Default::default()
        };

        let rules = git_rules_for_policy(&settings);

        assert_that!(&(rules.contains("pattern = [\"git\", \"add\"]"))).is_true();
        assert_that!(&(rules.contains("pattern = [\"git\", \"commit\"]"))).is_true();
        assert_that!(&(rules.contains("pattern = [\"git\", \"push\"]"))).is_true();
        assert_that!(&(rules.contains("pattern = [\"git\", \"push\", \"--force\"]"))).is_true();
        assert_that!(&(rules.contains("pattern = [\"git\", \"reset\"]"))).is_true();
        assert_that!(&(rules.contains("pattern = [\"git\", \"reset\", \"--hard\"]"))).is_true();
        assert_that!(&(rules.contains("decision = \"forbidden\""))).is_true();
    }

    fn project_settings() -> ProjectSettingsView {
        ProjectSettingsView {
            id: 7,
            project_id: 7,
            knowledge_directory: "knowledge".to_owned(),
            workspace_mode: WorkspaceMode::GitWorktree,
            max_code_edit_agents: 1,
            max_read_only_agents: 2,
            create_pr: false,
            auto_commit: true,
            commit_standard: String::new(),
            revert_strategy: RevertStrategy::Manual,
            stale_claim_minutes: 0,
            worktree_cleanup_policy: WorktreeCleanupPolicy::Manual,
            default_agent_tool: AgentToolName::Codex,
            default_agent_model: None,
            default_agent_reasoning_effort: Some(AgentReasoningEffort::XHigh),
            agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite,
            agent_extra_writable_roots: Vec::new(),
            agent_git_command_policy: AgentGitCommandPolicy::default(),
            created_at: "2026-06-17T00:00:00Z".to_owned(),
            updated_at: "2026-06-17T00:00:00Z".to_owned(),
        }
    }

    fn service_with_store(
        store: &Store,
        home: &Path,
        sessions: ProcessSessionRegistry,
    ) -> CodexService {
        CodexService::new(
            crate::backend::execution::tools::tests::service(store),
            home.to_path_buf(),
            crate::backend::events::UiEventBus::new(),
            sessions,
        )
    }
    async fn service(home: &Path, sessions: ProcessSessionRegistry) -> CodexService {
        let store = Store::open(home.join("dispatch.sqlite3")).await.unwrap();
        service_with_store(&store, home, sessions)
    }
}
