mod automation;
mod board;
mod board_drawer;
mod board_groups;
mod common;
mod item_detail;
mod knowledge;
mod navigation;
mod new_item;
mod project_administration;
mod project_lifecycle;
mod projects;
mod runs;
mod signal_shutdown;

use browser_test::BrowserTests;

pub(crate) use common::{DispatchTestApp, DispatchTestAppStartError, write_signal_probe};

pub(crate) fn tests() -> BrowserTests<DispatchTestApp> {
    if std::env::var_os("DISPATCH_BROWSER_SIGNAL_SCENARIO").is_some() {
        return BrowserTests::new().with(signal_shutdown::SignalShutdownTest);
    }

    BrowserTests::new()
        .with(projects::ProjectsAndSystemTest)
        .with(project_lifecycle::ProjectLifecycleTest)
        .with(board::BoardShellTest)
        .with(board_groups::BoardGroupsAndApiTest)
        .with(project_administration::ProjectAdministrationTest)
        .with(knowledge::KnowledgeReaderTest)
        .with(runs::RunLogsTest)
        .with(navigation::LocalNavigationTest)
        .with(automation::AutomationAdministrationTest)
        .with(new_item::NewItemWorkflowTest)
        .with(board_drawer::BoardDrawerTest)
        .with(item_detail::ItemDetailTest)
}
