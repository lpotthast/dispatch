use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};

use super::common::*;

pub(crate) struct ProjectsAndSystemTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for ProjectsAndSystemTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("projects and system surfaces render")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        open_projects_without_test_projects(driver, app).await?;
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to open Dispatch projects page")?;

        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Projects");
        find(driver, By::Css(".project-switcher")).await?;
        find(driver, By::Css(".workspace-dock")).await?;
        find(driver, By::Css("[data-crudkit-leptos='projects']")).await?;
        assert_main_content_scrolls_clear_of_workspace_dock(driver).await?;
        assert_source_contains(driver, "project-switcher").await?;
        assert_source_does_not_contain(driver, ">Switch<").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"projects\"").await?;
        assert_source_does_not_contain(driver, "Existing projects").await?;
        assert_source_does_not_contain(driver, "project-create-form").await?;
        assert_source_does_not_contain(driver, "Codex app-server").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"agent-tools\"").await?;
        find(driver, By::Css(".topbar-codex")).await?;
        assert_source_does_not_contain(driver, "codex-status-panel").await?;
        click(driver, By::Css(".topbar-codex")).await?;
        assert_that!(
            driver
                .title()
                .await
                .context("failed to read System page title")?
        )
        .is_equal_to("System");
        assert_source_contains(driver, "Codex app-server").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"agent-tools\"").await?;
        assert_source_does_not_contain(driver, "/agent-tools/create").await?;
        find(driver, By::Css(".top-nav a.active[href='/system']")).await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='agent-tools'] .crud-nav"),
        )
        .await?;
        find(driver, By::Css(".codex-status-panel")).await?;
        find(
            driver,
            By::Css(".system-page .codex-status-panel ~ .app-tools"),
        )
        .await?;
        find(
            driver,
            By::XPath(
                "//*[@data-crudkit-leptos='agent-tools']//button[normalize-space()='Check Codex']",
            ),
        )
        .await?;
        assert_that!(
            driver
                .find_all(By::Css(".app-tools > button"))
                .await
                .context("failed to inspect standalone app-tool actions")?
                .is_empty()
        )
        .is_true();
        assert_codex_auth_guide_when_blocked(driver).await?;
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to reopen Dispatch projects page after Codex status check")?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"agent-tools\"").await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
        )
        .await?;
        click(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav button"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] select.agent-model-select"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] select.agent-reasoning-select"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] option[value='gpt-5.6-sol']"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] option[value='max']"),
        )
        .await?;
        driver
            .goto(app.url("/projects"))
            .await
            .context("failed to reopen Dispatch projects page after create-view check")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
        )
        .await?;
        assert_source_does_not_contain(driver, "Invalid URL").await?;
        assert_source_does_not_contain(driver, "relative URL without a base").await?;

        Ok(())
    }
}

async fn assert_codex_auth_guide_when_blocked(driver: &WebDriver) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    if source.contains("Codex automation blocked") && source.contains("Not signed in") {
        for expected in [
            "Sign in to Codex",
            "CODEX_HOME=",
            "CODEX_SQLITE_HOME=",
            "Copy command",
            "Copy home",
            "Log out",
            "OPENAI_API_KEY",
        ] {
            if !source.contains(expected) {
                bail!("blocked Codex auth guide did not include {expected:?}");
            }
        }
        if source.contains("Install Codex and make sure") {
            bail!("blocked Codex auth guide unexpectedly included the install prompt");
        }
    }
    Ok(())
}
