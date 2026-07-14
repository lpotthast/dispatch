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
pub(crate) use automation::{
    AutomationService, apply_bundle_yaml, detach_automation_personality, detach_automation_rule,
    diff_bundle_yaml, explain_automation_route, export_bundle_yaml, list_installed_bundles,
    load_automation_personality_inspector, load_automation_rule_inspector, remove_installed_bundle,
    restore_automation_personality_revision, restore_automation_rule_revision,
    validate_bundle_yaml,
};
pub(crate) use board::BoardService;
pub(crate) use codex::CodexService;
pub(crate) use items::ItemService;
pub(crate) use projects::{ProjectService, project_cache};
pub(crate) use runs::RunService;

use leptos::prelude::{expect_context, provide_context};

pub(crate) fn provide_frontend_services() {
    projects::provide_project_cache();
    provide_context(ApiDocsService::production());
    provide_context(AutomationService::production());
    provide_context(BoardService::production());
    provide_context(CodexService::production());
    provide_context(ItemService::production());
    provide_context(ProjectService::production());
    provide_context(RunService::production());
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
