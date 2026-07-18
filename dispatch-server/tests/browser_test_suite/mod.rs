mod automation;
mod board;
mod board_drawer;
mod board_groups;
mod common;
mod item_detail;
mod new_item;
mod project_administration;
mod project_lifecycle;
mod projects;
mod runs;

use browser_test::BrowserTests;

pub(crate) use common::DispatchTestApp;

pub(crate) fn tests() -> BrowserTests<DispatchTestApp> {
    BrowserTests::new()
        .with(projects::ProjectsAndSystemTest)
        .with(project_lifecycle::ProjectLifecycleTest)
        .with(board::BoardShellTest)
        .with(board_groups::BoardGroupsAndApiTest)
        .with(project_administration::ProjectAdministrationTest)
        .with(runs::RunLogsTest)
        .with(automation::AutomationAdministrationTest)
        .with(new_item::NewItemWorkflowTest)
        .with(board_drawer::BoardDrawerTest)
        .with(item_detail::ItemDetailTest)
}
