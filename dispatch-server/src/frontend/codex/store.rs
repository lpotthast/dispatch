use super::types::*;
use crate::frontend::codex::service::CodexService;
use crate::frontend::queries::cache::QueryCache;
use dispatch_types::*;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct CodexStore {
    service: CodexService,
    status: QueryCache<(), CodexAppServerStatusView>,
    page: QueryCache<Option<String>, CodexReadiness>,
}

impl CodexStore {
    pub(crate) fn new(service: CodexService) -> Self {
        Self {
            service,
            status: QueryCache::persistent("dispatch.store.codex-status.v1"),
            page: QueryCache::persistent("dispatch.store.codex-page.v1"),
        }
    }

    pub(crate) fn cached_status(&self) -> Option<CodexAppServerStatusView> {
        self.status.get(&())
    }

    pub(crate) async fn load_status(&self) -> Result<CodexAppServerStatusView, ServerFnError> {
        let service = self.service.clone();
        let current = self.status.clone();
        self.status
            .load_retained(
                (),
                move || async move { service.load_status().await },
                move |value| {
                    current
                        .get_untracked(&())
                        .is_none_or(|current| current.checked_at <= value.checked_at)
                },
            )
            .await
    }

    pub(crate) fn seed_status(&self, value: CodexAppServerStatusView) {
        self.status.seed((), value);
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<CodexReadiness> {
        self.page.get(&selected_project.clone())
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<CodexReadiness> {
        self.page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<CodexReadiness, ServerFnError> {
        let service = self.service.clone();
        let status = self.status.clone();
        self.page
            .load(selected_project.clone(), move || async move {
                let page = service.load_page(selected_project).await?;
                remember_status(&status, &page.codex_status);
                Ok(CodexReadiness::from(page))
            })
            .await
    }

    pub(crate) fn seed_page(&self, selected_project: Option<String>, value: CodexReadiness) {
        remember_status(&self.status, &value.codex_status);
        self.page.seed(selected_project.clone(), value);
    }

    pub(crate) fn invalidate_page(&self, selected_project: Option<String>) {
        self.page.invalidate_key(&selected_project);
    }

    pub(crate) fn invalidate_status(&self) {
        self.status.invalidate_key(&());
    }

    pub(crate) fn clear_cache(&self) {
        self.status.clear();
        self.page.clear();
    }
}

fn remember_status(
    cache: &QueryCache<(), CodexAppServerStatusView>,
    status: &CodexAppServerStatusView,
) {
    if !status.checked_at.is_empty()
        && cache
            .get_untracked(&())
            .is_none_or(|current| current.checked_at <= status.checked_at)
    {
        let ticket = cache.begin(());
        cache.commit(ticket, status.clone());
    }
}

pub(crate) fn codex_store() -> CodexStore {
    leptos::prelude::expect_context()
}
