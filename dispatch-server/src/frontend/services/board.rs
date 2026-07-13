#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::{BoardItemsSection, BoardPage},
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct BoardService {
    load_page: ServiceRequest<Option<String>, BoardPage>,
    load_items: ServiceRequest<String, BoardItemsSection>,
    page_cache: Option<LocalStorageCache<BoardPage>>,
    items_cache: Option<LocalStorageCache<BoardItemsSection>>,
}

impl BoardService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<BoardPage> + Send + Sync + 'static,
        load_items: impl Fn(String) -> ServiceFuture<BoardItemsSection> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            load_items: ServiceRequest::new(load_items),
            page_cache: None,
            items_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_board_page(selected_project, api_base_url())),
            |project| Box::pin(load_board_items_section(project)),
        );
        service.page_cache = Some(LocalStorageCache::persistent("dispatch.query.board.v1"));
        service.items_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.board-items.v1",
        ));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<BoardPage> {
        self.page_cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<BoardPage> {
        self.page_cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<BoardPage, ServerFnError> {
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.page_cache {
            cache.store(&key, &page);
        }
        Ok(page)
    }

    pub(crate) fn cached_items(&self, project: &str) -> Option<BoardItemsSection> {
        self.items_cache?.get(&project)
    }

    pub(crate) fn cached_items_untracked(&self, project: &str) -> Option<BoardItemsSection> {
        self.items_cache?.get_untracked(&project)
    }

    pub(crate) async fn load_items(
        &self,
        project: String,
    ) -> Result<BoardItemsSection, ServerFnError> {
        let key = project.clone();
        let items = self.load_items.execute(project).await?;
        if let Some(cache) = self.items_cache {
            cache.store(&key, &items);
        }
        Ok(items)
    }
}

#[server(prefix = "/leptos")]
async fn load_board_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<BoardPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::board_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
        api_base_url,
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_board_items_section(project: String) -> Result<BoardItemsSection, ServerFnError> {
    let state = app_state::app_state();
    page_data::board_items_section(&state.store, &project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}
