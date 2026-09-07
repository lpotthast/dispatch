use crate::frontend::codex::types::CodexReadiness;
use crate::frontend::queries::query_resource;
use crate::{
    frontend::{
        agent_tools::AgentToolsPanel,
        codex::service::codex_service,
        components::selected_project_signal,
        live_events::{codex_event_matches, refetch_on_live_event},
        projects::workspace::copy_workspace_text,
    },
    shared::view_models::{
        CodexAppServerStatusView, CodexAuthSetupView, CodexLogPurgeResultView,
        CodexLogStorageStatusView, CodexRateLimitView, CodexUsageSummaryView,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_use::use_interval_fn;

const CODEX_STATUS_PAGE_REFRESH_INTERVAL_MS: u64 = 5 * 60 * 1000;

#[component]
pub fn PageSystem() -> impl IntoView {
    let selected_project = selected_project_signal();
    let store = crate::frontend::codex::store::codex_store();
    let api_base_url = crate::frontend::http::HttpService::get().api_base_url();
    let initial = store.cached_page_untracked(&selected_project.get_untracked());
    let store_for_cache = store.clone();
    let store_for_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let result = query_resource(
        initial,
        move || selected_project.get(),
        move |selected_project| store_for_cache.cached_page(selected_project),
        move |selected_project| {
            let store = store_for_load.clone();
            let selected_project = selected_project.clone();
            async move { store.load_page(selected_project).await }
        },
        move |key, value| store_for_seed.seed_page(key, value),
        move |key| store_for_refresh.invalidate_page(key),
    );
    let refresh = result.refresh;
    let (log_purge_outcome, set_log_purge_outcome) =
        signal(None::<Result<CodexLogPurgeResultView, String>>);
    let _poll = use_interval_fn(
        move || refresh.run(()),
        CODEX_STATUS_PAGE_REFRESH_INTERVAL_MS,
    );
    refetch_on_live_event(result.refresh, codex_event_matches);

    view! {
            <Title text="System"/>
            <div>
                <main class="page-shell system-page codex-page">
                    <section class="page-heading">
                        <h1>"System"</h1>
                        <p class="muted">
                            "System-wide runtime configuration and status. Detailed Codex status refreshes every five minutes while this page is open."
                        </p>
                    </section>
                    <div class="system-sections">
                        <Transition>
    {move || {
                            let page = result.value.get().unwrap_or_else(|| CodexReadiness {
                                selected_project: selected_project.get(),
                                codex_status: CodexAppServerStatusView::default(),
                            });
                            view! {
                                <CodexStatusContent
                                    page
                                    on_refreshed=result.refresh
                                    log_purge_outcome
                                    set_log_purge_outcome
                                />
                            }
                        }}
    </Transition>
    <crate::frontend::components::QueryFeedback pending=result.pending error=result.error refresh=result.refresh/>
                        <AgentToolsPanel api_base_url on_refreshed=result.refresh/>
                    </div>
                </main>
            </div>
        }
}

#[component]
fn CodexStatusContent(
    page: CodexReadiness,
    on_refreshed: Callback<()>,
    log_purge_outcome: ReadSignal<Option<Result<CodexLogPurgeResultView, String>>>,
    set_log_purge_outcome: WriteSignal<Option<Result<CodexLogPurgeResultView, String>>>,
) -> impl IntoView {
    let CodexReadiness {
        selected_project,
        codex_status,
    } = page;
    let _ = selected_project;
    let log_storage = codex_status.log_storage.clone();
    view! {
        <CodexStatusPanel status=codex_status on_refreshed/>
        <CodexLogStorageMaintenance
            status=log_storage
            on_refreshed
            outcome=log_purge_outcome
            set_outcome=set_log_purge_outcome
        />
    }
}

#[component]
fn CodexLogStorageMaintenance(
    status: CodexLogStorageStatusView,
    on_refreshed: Callback<()>,
    outcome: ReadSignal<Option<Result<CodexLogPurgeResultView, String>>>,
    set_outcome: WriteSignal<Option<Result<CodexLogPurgeResultView, String>>>,
) -> impl IntoView {
    let has_oversized_databases = !status.oversized_databases.is_empty();
    let has_scan_errors = !status.scan_errors.is_empty();
    let has_storage_warning = has_oversized_databases || has_scan_errors;
    let section_class = if has_scan_errors {
        "codex-log-storage codex-log-storage-danger"
    } else if has_oversized_databases {
        "codex-log-storage codex-log-storage-warning"
    } else {
        "codex-log-storage codex-log-storage-success"
    };
    let heading = if has_scan_errors {
        "Codex log storage check failed"
    } else if has_oversized_databases {
        "Oversized Codex logs"
    } else {
        "Codex log cleanup complete"
    };
    let threshold = human_file_size(status.threshold_bytes);
    let databases = status
        .oversized_databases
        .into_iter()
        .map(|database| {
            let size = human_file_size(database.size_bytes);
            view! {
                <li>
                    <code>{database.relative_path}</code>
                    <strong>{size}</strong>
                </li>
            }
        })
        .collect::<Vec<_>>();
    let scan_errors = status
        .scan_errors
        .into_iter()
        .map(|error| view! { <li>{error}</li> })
        .collect::<Vec<_>>();
    let service = codex_service();
    let purge_action = Action::new(move |_: &()| {
        set_outcome.set(None);
        let service = service.clone();
        async move {
            let result = service
                .purge_oversized_logs()
                .await
                .map_err(|error| error.to_string());
            set_outcome.set(Some(result));
            on_refreshed.run(());
        }
    });
    let pending = purge_action.pending();
    let purge = Callback::new(move |_| {
        if pending.get_untracked() || has_scan_errors || !has_oversized_databases {
            return;
        }
        purge_action.dispatch(());
    });
    let outcome_view = move || {
        outcome.get().map(|outcome| match outcome {
            Ok(result) => view! {
                <p class="codex-log-purge-result success">
                    "Removed " {result.removed_database_count} " log database "
                    {if result.removed_database_count == 1 { "family" } else { "families" }}
                    " and reclaimed " {human_file_size(result.reclaimed_bytes)} "."
                </p>
            }
            .into_any(),
            Err(error) => view! {
                <p class="codex-log-purge-result error-message">
                    <strong>"Cleanup failed: "</strong>{error}
                </p>
            }
            .into_any(),
        })
    };

    view! {
        <section
            class=section_class
            hidden=move || !has_storage_warning && outcome.get().is_none()
        >
            <div class="codex-log-storage-header">
                <div>
                    <h2>{heading}</h2>
                    {has_oversized_databases.then(|| view! {
                        <p>
                            "Each listed managed Codex log database exceeds the "
                            {threshold.clone()} " cleanup threshold. This warning does not block automation."
                        </p>
                    })}
                    {has_scan_errors.then(|| view! {
                        <p>
                            "Dispatch could not safely inspect every managed Codex log location. Cleanup is disabled, but automation remains available."
                        </p>
                    })}
                </div>
                {has_storage_warning.then(move || view! {
                    <button
                        type="button"
                        class="danger"
                        disabled=move || pending.get() || has_scan_errors || !has_oversized_databases
                        on:click=move |event| purge.run(event)
                    >
                        {move || if pending.get() {
                            "Purging oversized Codex logs..."
                        } else {
                            "Purge oversized Codex logs"
                        }}
                    </button>
                })}
            </div>
            {(!databases.is_empty()).then(|| view! {
                <ul class="codex-log-databases">{databases}</ul>
            })}
            {(!scan_errors.is_empty()).then(|| view! {
                <ul class="codex-log-scan-errors">{scan_errors}</ul>
            })}
            {outcome_view}
        </section>
    }
}

fn human_file_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[component]
fn CodexStatusPanel(status: CodexAppServerStatusView, on_refreshed: Callback<()>) -> impl IntoView {
    let status_class = if status.usable {
        "codex-status-ready"
    } else if status.available {
        "codex-status-blocked"
    } else {
        "codex-status-unavailable"
    };
    let heading = if status.usable {
        "Codex automation ready"
    } else if status.available {
        "Codex automation blocked"
    } else {
        "Codex app-server unavailable"
    };
    let badge = if status.usable {
        "Ready"
    } else if status.available {
        "Blocked"
    } else {
        "Unavailable"
    };
    let binary = status
        .binary_path
        .clone()
        .unwrap_or_else(|| "not resolved".to_owned());
    let auth_method = status
        .auth_method
        .as_deref()
        .map(auth_method_label)
        .unwrap_or_else(|| "Not signed in".to_owned());
    let account = status
        .account_label
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    let plan = status
        .plan_type
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    let payment = status
        .payment_model
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    let requires_auth = status
        .requires_openai_auth
        .map(|value| if value { "yes" } else { "no" })
        .unwrap_or("unknown");
    let preconditions = status
        .preconditions
        .clone()
        .into_iter()
        .map(|precondition| {
            view! {
                <li>
                    <span class=if precondition.ok {
                        "check-state ok"
                    } else {
                        "check-state failed"
                    }>
                        {if precondition.ok { "OK" } else { "Fail" }}
                    </span>
                    <span>
                        <strong>{precondition.name}</strong>
                        <span>{precondition.message}</span>
                    </span>
                </li>
            }
        })
        .collect::<Vec<_>>();
    let rate_limits = if status.rate_limits.is_empty() {
        view! { <p class="muted">"No rate-limit details reported."</p> }.into_any()
    } else {
        let limits = status
            .rate_limits
            .iter()
            .cloned()
            .map(|limit| view! { <RateLimitView limit/> })
            .collect::<Vec<_>>();
        view! { <div class="codex-rate-limits">{limits}</div> }.into_any()
    };
    let usage = status
        .usage_summary
        .clone()
        .map(|summary| view! { <UsageSummaryView summary/> }.into_any())
        .unwrap_or_else(|| {
            view! { <p class="muted">"No token usage summary reported."</p> }.into_any()
        });
    let warnings = if status.warnings.is_empty() {
        ().into_any()
    } else {
        let warnings = status
            .warnings
            .clone()
            .into_iter()
            .map(|warning| view! { <li>{warning}</li> })
            .collect::<Vec<_>>();
        view! { <ul class="codex-status-warnings">{warnings}</ul> }.into_any()
    };
    let install_prompt = (!status.available).then(|| {
        view! { <p class="codex-install-prompt">{status.install_prompt.clone()}</p> }
    });
    let auth_setup = status
        .auth_setup
        .clone()
        .map(|setup| view! { <CodexAuthSetup setup/> });
    let can_logout = status.available
        && status.auth_method.as_deref() != Some("apiKey")
        && (status.signed_in || status.auth_setup.is_some());
    let service = codex_service();
    let logout_service = service.clone();
    let refresh_action = Action::new(move |_: &()| {
        let service = service.clone();
        async move {
            if service.discover_agent_tools().await.is_ok() {
                on_refreshed.run(());
            }
        }
    });
    let refresh_pending = refresh_action.pending();
    let refresh = move |_| {
        if refresh_pending.get_untracked() {
            return;
        }
        refresh_action.dispatch(());
    };
    let logout_action = Action::new(move |_: &()| {
        let service = logout_service.clone();
        async move {
            if service.logout().await.is_ok() {
                on_refreshed.run(());
            }
        }
    });
    let logout_pending = logout_action.pending();
    let logout = move |_| {
        if logout_pending.get_untracked() {
            return;
        }
        logout_action.dispatch(());
    };

    view! {
        <section class=format!("codex-status-panel {status_class}")>
            <div class="codex-status-header">
                <div>
                    <h2>{heading}</h2>
                    <p>{status.message.clone()}</p>
                    {install_prompt}
                </div>
                <div class="codex-status-actions">
                    <span class="codex-status-badge">{badge}</span>
                    <button
                        type="button"
                        class="secondary"
                        disabled=move || refresh_pending.get()
                        on:click=refresh
                    >
                        "Refresh"
                    </button>
                    {can_logout.then(move || view! {
                        <button
                            type="button"
                            class="danger"
                            disabled=move || logout_pending.get()
                            on:click=logout
                        >
                            "Log out"
                        </button>
                    })}
                </div>
            </div>
            {auth_setup}
            <div class="codex-status-grid">
                <div>
                    <span>"Binary"</span>
                    <code>{binary}</code>
                </div>
                <div>
                    <span>"Auth"</span>
                    <strong>{auth_method}</strong>
                </div>
                <div>
                    <span>"Account"</span>
                    <strong>{account}</strong>
                </div>
                <div>
                    <span>"OpenAI auth required"</span>
                    <strong>{requires_auth}</strong>
                </div>
                <div>
                    <span>"Payment"</span>
                    <strong>{payment}</strong>
                </div>
                <div>
                    <span>"Plan"</span>
                    <strong>{plan}</strong>
                </div>
                <div>
                    <span>"Checked"</span>
                    <strong>{status.checked_at.clone()}</strong>
                </div>
            </div>
            <div class="codex-status-columns">
                <div>
                    <h3>"Preconditions"</h3>
                    <ul class="codex-preconditions">{preconditions}</ul>
                </div>
                <div>
                    <h3>"Limits"</h3>
                    {rate_limits}
                </div>
                <div>
                    <h3>"Usage"</h3>
                    {usage}
                </div>
            </div>
            {warnings}
        </section>
    }
    .into_any()
}

#[component]
fn CodexAuthSetup(setup: CodexAuthSetupView) -> impl IntoView {
    let command = setup.login_command.clone();
    let command_for_copy = command.clone();
    let home_for_copy = setup.codex_home_path.clone();
    let (copy_message, set_copy_message) = signal(None::<String>);

    view! {
        <div class="codex-auth-guide">
            <div class="codex-auth-guide-main">
                <div>
                    <h3>"Sign in to Codex"</h3>
                    <p>
                        "Run this command in a terminal. It writes credentials into Dispatch's managed Codex home."
                    </p>
                </div>
                <div class="codex-auth-actions">
                    <button
                        type="button"
                        class="secondary"
                        on:click=move |_| {
                            copy_workspace_text(
                                command_for_copy.clone(),
                                "Copied login command",
                                set_copy_message,
                            );
                        }
                    >
                        "Copy command"
                    </button>
                    <button
                        type="button"
                        class="secondary"
                        on:click=move |_| {
                            copy_workspace_text(
                                home_for_copy.clone(),
                                "Copied Codex home",
                                set_copy_message,
                            );
                        }
                    >
                        "Copy home"
                    </button>
                    {move || {
                        copy_message
                            .get()
                            .map(|message| view! { <span class="workspace-copy-status">{message}</span> })
                    }}
                </div>
            </div>
            <code class="codex-login-command">{command}</code>
            <div class="codex-auth-notes">
                <p>{setup.refresh_instruction}</p>
                <p>{setup.api_key_instruction}</p>
            </div>
        </div>
    }
    .into_any()
}

fn auth_method_label(method: &str) -> String {
    match method {
        "apiKey" => "API key".to_owned(),
        "chatgpt" => "ChatGPT".to_owned(),
        "amazonBedrock" => "Amazon Bedrock".to_owned(),
        method => method.to_owned(),
    }
}

#[component]
fn RateLimitView(limit: CodexRateLimitView) -> impl IntoView {
    let lines = rate_limit_lines(&limit)
        .into_iter()
        .map(|line| view! { <li>{line}</li> })
        .collect::<Vec<_>>();
    let reached = limit.reached_type.clone().map(|reached| {
        view! { <span class="check-state failed">{reached}</span> }
    });

    view! {
        <article class="codex-rate-limit">
            <div>
                <strong>{limit.label.clone()}</strong>
                {limit.plan_type.as_ref().map(|plan| view! {
                    <span class="muted">"plan " {plan.clone()}</span>
                })}
            </div>
            {reached}
            <ul>{lines}</ul>
        </article>
    }
    .into_any()
}

fn rate_limit_lines(limit: &CodexRateLimitView) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(line) = rate_window_line(
        "Primary",
        limit.primary_used_percent,
        limit.primary_window_minutes,
        limit.primary_resets_at.as_deref(),
    ) {
        lines.push(line);
    }
    if let Some(line) = rate_window_line(
        "Secondary",
        limit.secondary_used_percent,
        limit.secondary_window_minutes,
        limit.secondary_resets_at.as_deref(),
    ) {
        lines.push(line);
    }
    if let Some(remaining) = limit.individual_remaining_percent {
        let mut line = format!("{remaining}% individual budget remaining");
        if let (Some(used), Some(max)) = (&limit.individual_used, &limit.individual_limit) {
            line.push_str(&format!(" ({used} of {max})"));
        }
        if let Some(resets_at) = &limit.individual_resets_at {
            line.push_str(&format!(", resets {resets_at}"));
        }
        lines.push(line);
    }
    if limit.credits_balance.is_some()
        || limit.credits_has_credits.is_some()
        || limit.credits_unlimited.is_some()
    {
        let balance = limit
            .credits_balance
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());
        let has_credits = limit
            .credits_has_credits
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("unknown");
        let unlimited = limit
            .credits_unlimited
            .map(|value| if value { "yes" } else { "no" })
            .unwrap_or("unknown");
        lines.push(format!(
            "Credits balance {balance}; has credits {has_credits}; unlimited {unlimited}"
        ));
    }
    if lines.is_empty() {
        lines.push("No window details reported.".to_owned());
    }
    lines
}

fn rate_window_line(
    label: &str,
    used_percent: Option<i64>,
    window_minutes: Option<i64>,
    resets_at: Option<&str>,
) -> Option<String> {
    let used_percent = used_percent?;
    let mut line = format!("{label}: {used_percent}% used");
    if let Some(window_minutes) = window_minutes {
        line.push_str(&format!(" over {window_minutes} min"));
    }
    if let Some(resets_at) = resets_at {
        line.push_str(&format!(", resets {resets_at}"));
    }
    Some(line)
}

#[component]
fn UsageSummaryView(summary: CodexUsageSummaryView) -> impl IntoView {
    let mut rows = Vec::new();
    if let Some(value) = summary.lifetime_tokens {
        rows.push(("Lifetime tokens", format_number(value)));
    }
    if let Some(value) = summary.peak_daily_tokens {
        rows.push(("Peak daily tokens", format_number(value)));
    }
    if let Some(value) = summary.current_streak_days {
        rows.push(("Current streak", format!("{value} days")));
    }
    if let Some(value) = summary.longest_streak_days {
        rows.push(("Longest streak", format!("{value} days")));
    }
    if let Some(value) = summary.longest_running_turn_seconds {
        rows.push(("Longest turn", format!("{value} sec")));
    }
    if rows.is_empty() {
        return view! { <p class="muted">"No token usage summary reported."</p> }.into_any();
    }
    let rows = rows
        .into_iter()
        .map(|(label, value)| {
            view! {
                <div>
                    <span>{label}</span>
                    <strong>{value}</strong>
                </div>
            }
        })
        .collect::<Vec<_>>();
    view! { <div class="codex-usage-summary">{rows}</div> }.into_any()
}

pub(crate) fn format_number(value: i64) -> String {
    let absolute = if value < 0 {
        -(value as i128)
    } else {
        value as i128
    };
    let mut chars = absolute.to_string().chars().rev().collect::<Vec<_>>();
    let mut formatted = String::new();
    for (index, ch) in chars.drain(..).enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    let mut formatted = formatted.chars().rev().collect::<String>();
    if value < 0 {
        formatted.insert(0, '-');
    }
    formatted
}
