use super::types::*;
use crate::frontend::api_docs::service::ApiDocsService;
use crate::frontend::queries::cache::QueryCache;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ApiDocsStore {
    service: ApiDocsService,
    page: QueryCache<Option<String>, ApiDocsSelection>,
}

impl ApiDocsStore {
    pub(crate) fn new(service: ApiDocsService) -> Self {
        Self {
            service,
            page: QueryCache::persistent("dispatch.store.api-docs.v1"),
        }
    }

    pub(crate) fn cached_page(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ApiDocsSelection> {
        self.page.get(&selected_project.clone())
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ApiDocsSelection> {
        self.page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ApiDocsSelection, ServerFnError> {
        let service = self.service.clone();
        self.page
            .load(selected_project.clone(), move || async move {
                service.load_page(selected_project).await.map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_page(&self, selected_project: Option<String>, value: ApiDocsSelection) {
        self.page.seed(selected_project.clone(), value);
    }

    pub(crate) fn invalidate_page(&self, selected_project: Option<String>) {
        self.page.invalidate_key(&selected_project);
    }

    pub(crate) fn clear_cache(&self) {
        self.page.clear();
    }
}

pub(crate) fn api_docs_store() -> ApiDocsStore {
    leptos::prelude::expect_context()
}
