use std::{borrow::Cow, fs, fs::OpenOptions, path::Path, time::Duration};

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
        wait_for_initial_codex_status(driver).await?;
        let codex_home = app.temp_dir().join(".dispatch/codex");
        fs::create_dir_all(&codex_home).context("failed to create browser-test Codex home")?;
        let oversized_log = codex_home.join("logs_browser.sqlite");
        let oversized_wal = codex_home.join("logs_browser.sqlite-wal");
        let unrelated_state = codex_home.join("state_5.sqlite");
        let unrelated_log = codex_home.join("session.log");
        sparse_file(&oversized_log, 1024 * 1024 * 1024 + 1)?;
        sparse_file(&oversized_wal, 4096)?;
        fs::write(&unrelated_state, b"keep state")
            .context("failed to write browser-test unrelated state database")?;
        fs::write(&unrelated_log, b"keep session log")
            .context("failed to write browser-test unrelated text log")?;
        click(
            driver,
            By::XPath("//*[@class='codex-status-actions']//button[normalize-space()='Refresh']"),
        )
        .await?;
        wait_for_source_text(driver, "logs_browser.sqlite", true).await?;
        find(driver, By::Css(".codex-log-storage-warning")).await?;
        assert_source_contains(driver, "logs_browser.sqlite").await?;
        assert_source_contains(driver, "1.00 GiB").await?;
        click(
            driver,
            By::XPath("//button[normalize-space()='Purge oversized Codex logs']"),
        )
        .await?;
        wait_for_source_text(driver, "logs_browser.sqlite", false).await?;
        assert_that!(
            &driver
                .find_all(By::Css(".codex-log-storage-warning"))
                .await
                .context("failed to verify removal of Codex log storage warning")?
                .is_empty()
        )
        .is_true();
        assert_that!(&(!oversized_log.exists())).is_true();
        assert_that!(&(!oversized_wal.exists())).is_true();
        assert_that!(&(unrelated_state.exists())).is_true();
        assert_that!(&(unrelated_log.exists())).is_true();
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

fn sparse_file(path: &Path, size_bytes: u64) -> Result<(), Report> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .context("failed to create sparse Codex log fixture")?;
    file.set_len(size_bytes)
        .context("failed to size sparse Codex log fixture")?;
    Ok(())
}

async fn wait_for_initial_codex_status(driver: &WebDriver) -> Result<(), Report> {
    for _ in 0..200 {
        if let Ok(checked) = driver
            .find(By::Css(".codex-status-grid > div:last-child strong"))
            .await
            && !checked
                .text()
                .await
                .context("failed to read initial Codex status timestamp")?
                .trim()
                .is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("timed out waiting for initial Codex status");
}

async fn wait_for_source_text(
    driver: &WebDriver,
    expected: &str,
    present: bool,
) -> Result<(), Report> {
    for _ in 0..200 {
        let source = driver
            .source()
            .await
            .context("failed to inspect Codex log maintenance source")?;
        if source.contains(expected) == present {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!(
        "timed out waiting for page source to {} {expected:?}",
        if present { "contain" } else { "omit" }
    );
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
