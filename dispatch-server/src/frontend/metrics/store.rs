use super::types::*;
use crate::frontend::metrics::service::MetricsService;
use crate::frontend::queries::cache::QueryCache;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct MetricsStore {
    service: MetricsService,
    page: QueryCache<Option<String>, MetricsSnapshot>,
}

impl MetricsStore {
    pub(crate) fn new(service: MetricsService) -> Self {
        Self {
            service,
            page: QueryCache::in_memory(),
        }
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<MetricsSnapshot> {
        self.page.get(&selected_project.clone())
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<MetricsSnapshot> {
        self.page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<MetricsSnapshot, ServerFnError> {
        let service = self.service.clone();
        self.page
            .load(selected_project.clone(), move || async move {
                service.load_page(selected_project).await.map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_page(&self, selected_project: Option<String>, value: MetricsSnapshot) {
        self.page.seed(selected_project.clone(), value);
    }

    pub(crate) fn invalidate_page(&self, selected_project: Option<String>) {
        self.page.invalidate_key(&selected_project);
    }

    pub(crate) fn clear_cache(&self) {
        self.page.clear();
    }
}

pub(crate) fn metrics_store() -> MetricsStore {
    leptos::prelude::expect_context()
}
