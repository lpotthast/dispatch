use crate::frontend::api_docs::{service::ApiDocsService, store::ApiDocsStore};
use crate::frontend::automation::{service::AutomationService, store::AutomationStore};
use crate::frontend::board::{service::BoardService, store::BoardStore};
use crate::frontend::codex::{service::CodexService, store::CodexStore};
use crate::frontend::http::HttpService;
use crate::frontend::items::{service::ItemService, store::ItemStore};
use crate::frontend::knowledge::{service::KnowledgeUiService, store::KnowledgeStore};
use crate::frontend::metrics::{service::MetricsService, store::MetricsStore};
use crate::frontend::projects::{service::ProjectService, store::ProjectStore};
use crate::frontend::runs::{service::RunService, store::RunStore};
use leptos::prelude::*;
pub(crate) fn provide_frontend_services() {
    let http = HttpService::new();
    provide_context(http.clone());
    provide_context(ApiDocsService::production(http.clone()));
    provide_context(AutomationService::production(http.clone()));
    provide_context(BoardService::production(http.clone()));
    provide_context(CodexService::production(http.clone()));
    provide_context(ItemService::production(http.clone()));
    provide_context(KnowledgeUiService::production(http.clone()));
    provide_context(MetricsService::production(http.clone()));
    provide_context(ProjectService::production(http.clone()));
    provide_context(RunService::production(http.clone()));
}

#[derive(Clone, Copy)]
pub(crate) struct ProjectLifecycleEpoch(pub(crate) RwSignal<u64>);

pub(crate) fn provide_frontend_stores() {
    provide_context(ProjectLifecycleEpoch(RwSignal::new(0)));
    provide_context(ApiDocsStore::new(
        crate::frontend::api_docs::service::api_docs_service(),
    ));
    provide_context(AutomationStore::new(
        crate::frontend::automation::service::automation_service(),
    ));
    provide_context(BoardStore::new(
        crate::frontend::board::service::board_service(),
    ));
    provide_context(CodexStore::new(
        crate::frontend::codex::service::codex_service(),
    ));
    provide_context(ItemStore::new(
        crate::frontend::items::service::item_service(),
    ));
    provide_context(KnowledgeStore::new(
        crate::frontend::knowledge::service::knowledge_ui_service(),
    ));
    provide_context(MetricsStore::new(
        crate::frontend::metrics::service::metrics_service(),
    ));
    provide_context(ProjectStore::new(
        crate::frontend::projects::service::project_service(),
    ));
    provide_context(RunStore::new(crate::frontend::runs::service::run_service()));
}

pub(crate) fn project_lifecycle_epoch_signal() -> RwSignal<u64> {
    expect_context::<ProjectLifecycleEpoch>().0
}

#[cfg(not(feature = "ssr"))]
pub(crate) fn advance_project_lifecycle_epoch() {
    expect_context::<ProjectLifecycleEpoch>()
        .0
        .update(|epoch| *epoch = epoch.wrapping_add(1));
}
