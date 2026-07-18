#[cfg(feature = "ssr")]
use std::time::Duration;

#[cfg(feature = "ssr")]
use crate::backend::{agent_tools, app_state, codex_app_server, events, page_data};
use crate::frontend::{
    pages::CodexStatusPage,
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[cfg(feature = "ssr")]
const CODEX_STATUS_PAGE_MINIMUM_REFRESH_AGE: Duration = Duration::from_secs(4 * 60);

#[derive(Clone)]
pub(crate) struct CodexService {
    load_page: ServiceRequest<Option<String>, CodexStatusPage>,
    discover_agent_tools: ServiceRequest<(), ()>,
    logout: ServiceRequest<(), ()>,
    cache: Option<LocalStorageCache<CodexStatusPage>>,
    crudkit_api_base_url: String,
}

impl CodexService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<CodexStatusPage> + Send + Sync + 'static,
        discover_agent_tools: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        logout: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        crudkit_api_base_url: String,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            discover_agent_tools: ServiceRequest::new(discover_agent_tools),
            logout: ServiceRequest::new(logout),
            cache: None,
            crudkit_api_base_url,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_codex_status_page(selected_project)),
            |()| Box::pin(discover_agent_tools()),
            |()| Box::pin(logout()),
            api_base_url(),
        );
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.codex.v1"));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<CodexStatusPage> {
        self.cache?.get(&("page", selected_project))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<CodexStatusPage> {
        self.cache?.get_untracked(&("page", selected_project))
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<CodexStatusPage, ServerFnError> {
        let lifecycle_epoch = self.cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.cache
        {
            cache.store(&("page", key), &page);
        }
        Ok(page)
    }

    pub(crate) async fn discover_agent_tools(&self) -> Result<(), ServerFnError> {
        self.discover_agent_tools.execute(()).await
    }

    pub(crate) async fn logout(&self) -> Result<(), ServerFnError> {
        self.logout.execute(()).await
    }

    pub(crate) fn crudkit_api_base_url(&self) -> &str {
        &self.crudkit_api_base_url
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.cache {
            cache.clear();
        }
    }
}

#[server(prefix = "/leptos")]
async fn load_codex_status_page(
    selected_project: Option<String>,
) -> Result<CodexStatusPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state
        .codex_status_refresh
        .refresh_if_stale(
            &state.store,
            &state.codex_status,
            CODEX_STATUS_PAGE_MINIMUM_REFRESH_AGE,
        )
        .await;
    page_data::codex_status_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn discover_agent_tools() -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    agent_tools::discover_tools(&state.store)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))?;
    state
        .codex_status_refresh
        .refresh_now(&state.store, &state.codex_status)
        .await;
    events::publish_agent_tool_changed();
    events::publish_codex_status_changed();
    Ok(())
}

#[server(prefix = "/leptos")]
async fn logout() -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    let status = codex_app_server::logout_current_account(&state.store)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))?;
    state
        .codex_status_refresh
        .store_detailed(&state.codex_status, status)
        .await;
    events::publish_codex_status_changed();
    Ok(())
}
