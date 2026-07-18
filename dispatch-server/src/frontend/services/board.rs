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
        service.page_cache = Some(LocalStorageCache::persistent("dispatch.query.board.v2"));
        service.items_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.board-items.v2",
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
        let lifecycle_epoch = self.page_cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.page_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) {
            if let (Some(cache), Some((project, section))) =
                (self.items_cache, board_items_section_from_page(&page))
            {
                cache.store(&project, &section);
            }
            if let Some(cache) = self.page_cache {
                cache.store(&key, &page);
            }
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
        let lifecycle_epoch = self
            .items_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let items = self.load_items.execute(project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.items_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.items_cache
        {
            cache.store(&key, &items);
        }
        Ok(items)
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.page_cache {
            cache.clear();
        }
        if let Some(cache) = self.items_cache {
            cache.clear();
        }
    }
}

fn board_items_section_from_page(page: &BoardPage) -> Option<(String, BoardItemsSection)> {
    Some((
        page.selected_project.clone()?,
        BoardItemsSection {
            items: page.items.clone(),
            swim_lanes: page.swim_lanes.clone(),
            work_item_states: page.work_item_states.clone(),
            misconfigured_item_count: page.misconfigured_item_count,
        },
    ))
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
