#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::ItemPage,
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ItemService {
    load_page: ServiceRequest<(Option<String>, Option<i64>), ItemPage>,
    cache: Option<LocalStorageCache<ItemPage>>,
}

impl ItemService {
    pub(crate) fn new(
        load_page: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<ItemPage>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(|(project, item_id)| {
            Box::pin(load_item_page(project, item_id, api_base_url()))
        });
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.items.v1"));
        service
    }

    pub(crate) fn cached_page(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemPage> {
        self.cache?.get(&("page", project, item_id))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemPage> {
        self.cache?.get_untracked(&("page", project, item_id))
    }

    pub(crate) async fn load_page(
        &self,
        project: Option<String>,
        item_id: Option<i64>,
    ) -> Result<ItemPage, ServerFnError> {
        let key = project.clone();
        let page = self.load_page.execute((project, item_id)).await?;
        if let Some(cache) = self.cache {
            cache.store(&("page", key, item_id), &page);
        }
        Ok(page)
    }
}

#[server(prefix = "/leptos")]
async fn load_item_page(
    project: Option<String>,
    item_id: Option<i64>,
    api_base_url: String,
) -> Result<ItemPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    match (project, item_id) {
        (Some(project), Some(item_id)) => page_data::item_page_data(
            &state.store,
            &state.automation_controller,
            &project,
            item_id,
            api_base_url,
            codex_status,
        )
        .await
        .map_err(|err| ServerFnError::new(err.to_string())),
        _ => Err(ServerFnError::new("Missing item route parameters")),
    }
}
