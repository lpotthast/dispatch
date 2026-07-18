mod api_docs;
mod automation;
mod board;
mod cache;
mod codex;
mod items;
mod origin;
mod projects;
mod request;
mod runs;

pub(crate) use api_docs::ApiDocsService;
pub(crate) use automation::AutomationService;
pub(crate) use board::BoardService;
pub(crate) use codex::CodexService;
pub(crate) use items::ItemService;
pub(crate) use projects::{CommitPolicyUpdate, ProjectService, project_cache};
pub(crate) use runs::RunService;

use leptos::prelude::*;

#[derive(Clone, Copy)]
struct ProjectLifecycleEpoch(RwSignal<u64>);

pub(crate) fn provide_frontend_services() {
    provide_context(ProjectLifecycleEpoch(RwSignal::new(0)));
    projects::provide_project_cache();
    provide_context(ApiDocsService::production());
    provide_context(AutomationService::production());
    provide_context(BoardService::production());
    provide_context(CodexService::production());
    provide_context(ItemService::production());
    provide_context(ProjectService::production());
    provide_context(RunService::production());
}

fn project_lifecycle_epoch_signal() -> RwSignal<u64> {
    expect_context::<ProjectLifecycleEpoch>().0
}

#[cfg(not(feature = "ssr"))]
pub(crate) fn advance_project_lifecycle_epoch() {
    expect_context::<ProjectLifecycleEpoch>()
        .0
        .update(|epoch| *epoch = epoch.wrapping_add(1));
}

pub(crate) fn api_docs_service() -> ApiDocsService {
    expect_context()
}

pub(crate) fn automation_service() -> AutomationService {
    expect_context()
}

pub(crate) fn board_service() -> BoardService {
    expect_context()
}

pub(crate) fn codex_service() -> CodexService {
    expect_context()
}

pub(crate) fn item_service() -> ItemService {
    expect_context()
}

pub(crate) fn project_service() -> ProjectService {
    expect_context()
}

pub(crate) fn run_service() -> RunService {
    expect_context()
}
