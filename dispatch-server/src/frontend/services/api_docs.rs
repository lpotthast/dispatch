#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::ApiDocsPage,
    services::{
        cache::LocalStorageCache,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ApiDocsService {
    load_page: ServiceRequest<Option<String>, ApiDocsPage>,
    cache: Option<LocalStorageCache<ApiDocsPage>>,
}

impl ApiDocsService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<ApiDocsPage> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service =
            Self::new(|selected_project| Box::pin(load_api_docs_page(selected_project)));
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.api-docs.v1"));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<ApiDocsPage> {
        self.cache?.get(&("page", selected_project))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ApiDocsPage> {
        self.cache?.get_untracked(&("page", selected_project))
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ApiDocsPage, ServerFnError> {
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.cache {
            cache.store(&("page", key), &page);
        }
        Ok(page)
    }
}

#[server(prefix = "/leptos")]
async fn load_api_docs_page(
    selected_project: Option<String>,
) -> Result<ApiDocsPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::api_docs_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}
