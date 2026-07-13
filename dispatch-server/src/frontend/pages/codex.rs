use crate::{
    frontend::{
        components::{
            ActivePage, cached_query, copy_workspace_text, encode_path, selected_project_signal,
            top_bar,
        },
        live_events::{codex_event_matches, refetch_on_live_event},
        services::{codex_service, project_cache},
    },
    shared::view_models::{
        CodexAppServerStatusView, CodexAuthSetupView, CodexRateLimitView, CodexUsageSummaryView,
        ProjectView,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_use::use_interval_fn;
use serde::{Deserialize, Serialize};

const CODEX_STATUS_PAGE_REFRESH_INTERVAL_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodexStatusPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageCodex() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = codex_service();
    let initial = service.cached_page_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |selected_project| service_for_cache.cached_page(selected_project),
        move |selected_project| {
            let service = service_for_load.clone();
            let selected_project = selected_project.clone();
            async move { service.load_page(selected_project).await }
        },
    );
    let refresh = result.refresh;
    let _poll = use_interval_fn(
        move || refresh.run(()),
        CODEX_STATUS_PAGE_REFRESH_INTERVAL_MS,
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, codex_event_matches);
    let active_project_names = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.active_project_names)
            .unwrap_or_default()
    });
    let codex_status = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.codex_status)
            .unwrap_or_default()
    });
    let topbar = top_bar(
        active_project_names,
        selected_project.into(),
        ActivePage::Codex,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Codex automation"/>
        <div>
            {topbar}
            <main class="page-shell codex-page">
                <section class="page-heading">
                    <h1>"Codex automation"</h1>
                    <p class="muted">
                        "Detailed status refreshes every five minutes while this page is open."
                    </p>
                </section>
                {move || {
                    let page = result.value.get().unwrap_or_else(|| CodexStatusPage {
                        projects: Vec::new(),
                        active_project_names: Vec::new(),
                        selected_project: selected_project.get(),
                        codex_status: CodexAppServerStatusView::default(),
                    });
                    codex_status_content(page)
                }}
            </main>
        </div>
    }
}

fn codex_status_content(page: CodexStatusPage) -> AnyView {
    let CodexStatusPage {
        projects: _,
        active_project_names: _,
        selected_project,
        codex_status,
    } = page;
    let return_to = selected_project
        .as_deref()
        .map(|project| format!("/codex?project={}", encode_path(project)))
        .unwrap_or_else(|| "/codex".to_owned());
    codex_status_panel(&codex_status, return_to)
}

fn codex_status_panel(status: &CodexAppServerStatusView, return_to: String) -> AnyView {
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
            .map(rate_limit_view)
            .collect::<Vec<_>>();
        view! { <div class="codex-rate-limits">{limits}</div> }.into_any()
    };
    let usage = status
        .usage_summary
        .as_ref()
        .map(usage_summary_view)
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
    let auth_setup = status.auth_setup.clone().map(codex_auth_setup_view);
    let can_logout = status.available
        && status.auth_method.as_deref() != Some("apiKey")
        && (status.signed_in || status.auth_setup.is_some());
    let return_to_for_refresh = return_to.clone();
    let return_to_for_logout = return_to;
    let logout_action = can_logout.then(|| {
        view! {
            <form method="post" action="/codex/logout">
                <input type="hidden" name="return_to" value=return_to_for_logout/>
                <button type="submit" class="danger">"Log out"</button>
            </form>
        }
    });

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
                    <form method="post" action="/agent-tools/discover">
                        <input type="hidden" name="return_to" value=return_to_for_refresh/>
                        <button type="submit" class="secondary">"Refresh"</button>
                    </form>
                    {logout_action}
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

fn codex_auth_setup_view(setup: CodexAuthSetupView) -> AnyView {
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

fn rate_limit_view(limit: &CodexRateLimitView) -> AnyView {
    let lines = rate_limit_lines(limit)
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

fn usage_summary_view(summary: &CodexUsageSummaryView) -> AnyView {
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
