#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::MetricsPageData,
    services::{
        cache::LocalStorageCache,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct MetricsService {
    load_page: ServiceRequest<Option<String>, MetricsPageData>,
    cache: Option<LocalStorageCache<MetricsPageData>>,
}

impl MetricsService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<MetricsPageData> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service =
            Self::new(|selected_project| Box::pin(load_metrics_page(selected_project)));
        service.cache = Some(LocalStorageCache::in_memory());
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<MetricsPageData> {
        self.cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<MetricsPageData> {
        self.cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<MetricsPageData, ServerFnError> {
        let lifecycle_epoch = self.cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.cache
        {
            cache.store(&key, &page);
        }
        Ok(page)
    }
}

#[server(prefix = "/leptos")]
async fn load_metrics_page(
    selected_project: Option<String>,
) -> Result<MetricsPageData, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    crate::backend::metrics::time_repository(
        "metrics.page",
        page_data::metrics_page_data(
            &state.store,
            &state.automation_controller,
            codex_status,
            selected_project.as_deref(),
        ),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}
