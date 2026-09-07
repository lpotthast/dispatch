use super::types::*;
use crate::frontend::board::service::BoardService;
use crate::frontend::queries::cache::QueryCache;
use dispatch_types::*;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct BoardStore {
    service: BoardService,
    page: QueryCache<Option<String>, BoardShell>,
    items: QueryCache<String, BoardItemsSection>,
}

impl BoardStore {
    pub(crate) fn new(service: BoardService) -> Self {
        Self {
            service,
            page: QueryCache::persistent("dispatch.store.board.v1"),
            items: QueryCache::persistent("dispatch.store.board-items.v1"),
        }
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<BoardShell> {
        self.page.get(&selected_project.clone())
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<BoardShell> {
        self.page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<BoardShell, ServerFnError> {
        let service = self.service.clone();
        let items = self.items.clone();
        self.page
            .load(selected_project.clone(), move || async move {
                let ticket = selected_project.clone().map(|project| items.begin(project));
                let page = service.load_page(selected_project).await?;
                if let Some(ticket) = ticket {
                    items.commit(ticket, board_items(&page));
                }
                Ok(BoardShell::from(page))
            })
            .await
    }

    pub(crate) fn seed_page(&self, selected_project: Option<String>, value: BoardShell) {
        self.page.seed(selected_project.clone(), value);
    }

    pub(crate) fn cached_items(&self, project: &str) -> Option<BoardItemsSection> {
        self.items.get(&project.to_owned())
    }

    pub(crate) fn cached_items_untracked(&self, project: &str) -> Option<BoardItemsSection> {
        self.items.get_untracked(&project.to_owned())
    }

    pub(crate) async fn load_items(
        &self,
        project: String,
    ) -> Result<BoardItemsSection, ServerFnError> {
        let service = self.service.clone();
        self.items
            .load(project.clone(), move || async move {
                service.load_items(project).await
            })
            .await
    }

    pub(crate) fn seed_items(&self, project: String, value: BoardItemsSection) {
        self.items.seed(project.clone(), value);
    }

    pub(crate) fn invalidate_page(&self, selected_project: Option<String>) {
        self.page.invalidate_key(&selected_project);
    }

    pub(crate) fn invalidate_items(&self, project: String) {
        self.items.invalidate_key(&project);
    }

    pub(crate) fn clear_cache(&self) {
        self.page.clear();
        self.items.clear();
    }
}

fn board_items(page: &BoardPage) -> BoardItemsSection {
    BoardItemsSection {
        items: page.items.clone(),
        swim_lanes: page.swim_lanes.clone(),
        work_item_states: page.work_item_states.clone(),
        label_accent_colors: page.label_accent_colors.clone(),
        misconfigured_item_count: page.misconfigured_item_count,
    }
}

pub(crate) fn board_store() -> BoardStore {
    leptos::prelude::expect_context()
}
