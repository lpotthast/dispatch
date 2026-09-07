use super::types::*;
use crate::frontend::items::service::ItemService;
use crate::frontend::queries::cache::QueryCache;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ItemStore {
    service: ItemService,
    page: QueryCache<(Option<String>, Option<i64>), ItemDetail>,
}

impl ItemStore {
    pub(crate) fn new(service: ItemService) -> Self {
        Self {
            service,
            page: QueryCache::persistent("dispatch.store.items.v1"),
        }
    }

    pub(crate) fn cached_page(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemDetail> {
        self.page.get(&(project.clone(), item_id))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemDetail> {
        self.page.get_untracked(&(project.clone(), item_id))
    }

    pub(crate) async fn load_page(
        &self,
        project: Option<String>,
        item_id: Option<i64>,
    ) -> Result<ItemDetail, ServerFnError> {
        let service = self.service.clone();
        self.page
            .load((project.clone(), item_id), move || async move {
                service.load_page(project, item_id).await.map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_page(
        &self,
        project: Option<String>,
        item_id: Option<i64>,
        value: ItemDetail,
    ) {
        self.page.seed((project.clone(), item_id), value);
    }

    pub(crate) fn invalidate_page(&self, project: Option<String>, item_id: Option<i64>) {
        self.page.invalidate_key(&(project, item_id));
    }

    pub(crate) fn clear_cache(&self) {
        self.page.clear();
    }
}

pub(crate) fn item_store() -> ItemStore {
    leptos::prelude::expect_context()
}
