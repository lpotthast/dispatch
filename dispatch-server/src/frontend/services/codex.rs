use crate::shared::page_data::CodexStatusPage;

#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::services::{
    cache::LocalStorageCache,
    origin::api_base_url,
    request::{ServiceFuture, ServiceRequest},
};
use leptos::prelude::*;

use crate::shared::view_models::{CodexAppServerStatusView, CodexLogPurgeResultView};

#[derive(Clone)]
pub(crate) struct CodexService {
    status_cache: Option<LocalStorageCache<CodexAppServerStatusView>>,
    load_status: ServiceRequest<(), CodexAppServerStatusView>,
    load_page: ServiceRequest<Option<String>, CodexStatusPage>,
    discover_agent_tools: ServiceRequest<(), ()>,
    logout: ServiceRequest<(), ()>,
    purge_oversized_logs: ServiceRequest<(), CodexLogPurgeResultView>,
    cache: Option<LocalStorageCache<CodexStatusPage>>,
    crudkit_api_base_url: String,
}

impl CodexService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<CodexStatusPage> + Send + Sync + 'static,
        discover_agent_tools: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        logout: impl Fn(()) -> ServiceFuture<()> + Send + Sync + 'static,
        purge_oversized_logs: impl Fn(()) -> ServiceFuture<CodexLogPurgeResultView>
        + Send
        + Sync
        + 'static,
        crudkit_api_base_url: String,
    ) -> Self {
        Self {
            status_cache: None,
            load_status: ServiceRequest::new(|()| Box::pin(load_codex_status())),
            load_page: ServiceRequest::new(load_page),
            discover_agent_tools: ServiceRequest::new(discover_agent_tools),
            logout: ServiceRequest::new(logout),
            purge_oversized_logs: ServiceRequest::new(purge_oversized_logs),
            cache: None,
            crudkit_api_base_url,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_codex_status_page(selected_project)),
            |()| Box::pin(discover_agent_tools()),
            |()| Box::pin(logout()),
            |()| Box::pin(purge_oversized_logs()),
            api_base_url(),
        );
        service.status_cache = Some(LocalStorageCache::persistent("dispatch.codex-status.v1"));
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.codex.v1"));
        service
    }

    pub(crate) fn cached_status(&self) -> Option<CodexAppServerStatusView> {
        self.status_cache?.get(&"status")
    }

    fn remember_status(&self, status: &CodexAppServerStatusView) {
        if status.checked_at.is_empty() {
            return;
        }
        if let Some(cache) = self.status_cache {
            // Route caches can be older than the latest live status.
            if cache
                .get_untracked(&"status")
                .is_none_or(|current| current.checked_at <= status.checked_at)
            {
                cache.store(&"status", status);
            }
        }
    }

    pub(crate) async fn load_status(&self) -> Result<CodexAppServerStatusView, ServerFnError> {
        let status = self.load_status.execute(()).await?;
        self.remember_status(&status);
        Ok(status)
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
        self.remember_status(&page.codex_status);
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

    pub(crate) async fn purge_oversized_logs(
        &self,
    ) -> Result<CodexLogPurgeResultView, ServerFnError> {
        self.purge_oversized_logs.execute(()).await
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

#[cfg(test)]
mod status_cache_tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn route_defaults_and_old_pages_do_not_replace_shared_codex_status() {
        Owner::new().with(|| {
            crate::frontend::services::provide_frontend_services();
            let service = crate::frontend::services::codex_service();
            let current = CodexAppServerStatusView {
                available: true,
                usable: true,
                checked_at: "2026-09-05T12:00:00Z".to_owned(),
                ..Default::default()
            };
            service.remember_status(&current);
            service.remember_status(&CodexAppServerStatusView::default());
            service.remember_status(&CodexAppServerStatusView {
                checked_at: "2026-09-05T11:00:00Z".to_owned(),
                ..Default::default()
            });
            assert_that!(&service.cached_status()).is_equal_to(Some(current));
        });
    }
}
