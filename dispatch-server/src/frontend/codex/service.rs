use dispatch_types::CodexStatusPage;

#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::request::{ServiceFuture, ServiceRequest};
use leptos::prelude::*;

use crate::shared::view_models::{CodexAppServerStatusView, CodexLogPurgeResultView};

#[derive(Clone)]
pub(crate) struct CodexService {
    load_status: ServiceRequest<(), CodexAppServerStatusView>,
    load_page: ServiceRequest<Option<String>, CodexStatusPage>,
    discover_agent_tools: ServiceRequest<(), ()>,
    logout: ServiceRequest<(), ()>,
    purge_oversized_logs: ServiceRequest<(), CodexLogPurgeResultView>,
}

impl CodexService {
    pub(crate) fn new(
        http: crate::frontend::http::HttpService,
        load_page: impl Fn(Option<String>) -> ServiceFuture<CodexStatusPage> + Send + Sync + 'static,
        discover_agent_tools: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        logout: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        purge_oversized_logs: impl Fn(()) -> ServiceFuture<CodexLogPurgeResultView>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            load_status: http.request(|()| Box::pin(load_codex_status())),
            load_page: http.request(load_page),
            discover_agent_tools: http.request(discover_agent_tools),
            logout: http.request(logout),
            purge_oversized_logs: http.request(purge_oversized_logs),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(
            http.clone(),
            |selected_project| Box::pin(load_codex_status_page(selected_project)),
            |()| Box::pin(discover_agent_tools()),
            |()| Box::pin(logout()),
            |()| Box::pin(purge_oversized_logs()),
        )
    }

    pub(crate) async fn load_status(&self) -> Result<CodexAppServerStatusView, ServerFnError> {
        self.load_status.execute(()).await
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<CodexStatusPage, ServerFnError> {
        self.load_page.execute(selected_project).await
    }

    pub(crate) async fn discover_agent_tools(&self) -> Result<(), ServerFnError> {
        self.discover_agent_tools.execute(()).await
    }

    pub(crate) async fn logout(&self) -> Result<(), ServerFnError> {
        self.logout.execute(()).await
    }

    pub(crate) async fn purge_oversized_logs(
        &self,
    ) -> Result<CodexLogPurgeResultView, ServerFnError> {
        self.purge_oversized_logs.execute(()).await
    }
}

#[server(prefix = "/leptos")]
async fn load_codex_status() -> Result<CodexAppServerStatusView, ServerFnError> {
    // Reading the already computed status never starts an app-server readiness probe.
    Ok(leptos::prelude::expect_context::<app_state::AppState>()
        .codex_status
        .read()
        .await
        .clone())
}

#[server(prefix = "/leptos")]
async fn load_codex_status_page(
    selected_project: Option<String>,
) -> Result<CodexStatusPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .codex_page(selected_project.as_deref())
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn discover_agent_tools() -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .codex
        .discover_tools(&state.codex_status)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn logout() -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .codex
        .logout(&state.codex_status)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn purge_oversized_logs() -> Result<CodexLogPurgeResultView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .codex
        .purge_logs(&state.codex_status)
        .await
        .map_err(|error| ServerFnError::new(format!("{error:#}")))
}

pub(crate) fn codex_service() -> CodexService {
    leptos::prelude::expect_context()
}
