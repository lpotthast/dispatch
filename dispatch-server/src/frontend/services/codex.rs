#[cfg(feature = "ssr")]
use std::time::Duration;

#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::CodexStatusPage,
    services::{
        cache::LocalStorageCache,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[cfg(feature = "ssr")]
const CODEX_STATUS_PAGE_MINIMUM_REFRESH_AGE: Duration = Duration::from_secs(4 * 60);

#[derive(Clone)]
pub(crate) struct CodexService {
    load_page: ServiceRequest<Option<String>, CodexStatusPage>,
    cache: Option<LocalStorageCache<CodexStatusPage>>,
}

impl CodexService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<CodexStatusPage> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service =
            Self::new(|selected_project| Box::pin(load_codex_status_page(selected_project)));
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
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.cache {
            cache.store(&("page", key), &page);
        }
        Ok(page)
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
