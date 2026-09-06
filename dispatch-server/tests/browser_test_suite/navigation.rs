use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt};
use serde_json::json;

use super::common::*;

pub(crate) struct LocalNavigationTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for LocalNavigationTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("route parameters and run selection update locally")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        let first = create_browser_test_item(driver, "First route item", "").await?;
        let second = create_browser_test_item(driver, "Second route item", "").await?;
        driver
            .goto(app.url(&format!("/projects/demo/items/{first}")))
            .await?;
        find(driver, By::Css(".item-settings")).await?;
        let topbar = find(driver, By::Css(".app-topbar")).await?;
        click_route(
            driver,
            ".item-board-link",
            &format!("/projects/demo/items/{second}"),
        )
        .await?;
        find(driver, By::XPath("//h1[contains(., 'Second route item')]")).await?;
        assert_that!(find(driver, By::Css(".app-topbar")).await?.element_id())
            .is_equal_to(topbar.element_id());
        driver.back().await?;
        find(driver, By::XPath("//h1[contains(., 'First route item')]")).await?;
        driver.forward().await?;
        find(driver, By::XPath("//h1[contains(., 'Second route item')]")).await?;

        seed_run_commit_outcome_fixtures(app).await?;
        driver
            .goto(app.url("/projects/demo/automation/runs/501/log"))
            .await?;
        find(driver, By::Css(".run-result-inline.status-completed")).await?;
        let topbar = find(driver, By::Css(".app-topbar")).await?;
        click_route(
            driver,
            ".run-log .item-header a",
            "/projects/demo/automation/runs/502/log",
        )
        .await?;
        find(driver, By::XPath("//h1[normalize-space()='Run #502']")).await?;
        find(driver, By::Css(".run-result-inline.status-failed")).await?;
        assert_that!(find(driver, By::Css(".app-topbar")).await?.element_id())
            .is_equal_to(topbar.element_id());

        // A warmed record must render without waiting for server revalidation.
        driver.cdp().send_raw("Network.enable", json!({})).await?;
        driver
            .cdp()
            .send_raw(
                "Network.setBlockedURLs",
                json!({
                    "urls": ["*/leptos/*"]
                }),
            )
            .await?;
        let cached_navigation = async {
            click_route(
                driver,
                ".run-log .item-header a",
                "/projects/demo/automation/runs/501/log",
            )
            .await?;
            find(driver, By::XPath("//h1[normalize-space()='Run #501']")).await?;
            find(driver, By::Css(".run-result-inline.status-completed")).await?;
            assert_that!(find(driver, By::Css(".app-topbar")).await?.element_id())
                .is_equal_to(topbar.element_id());
            Ok::<_, Report>(())
        }
        .await;
        driver
            .cdp()
            .send_raw("Network.setBlockedURLs", json!({"urls": []}))
            .await?;
        cached_navigation?;

        driver.goto(app.url("/runs?project=demo&run=501")).await?;
        let row = find(
            driver,
            By::XPath(
                "//button[contains(@class, 'run-session')][.//strong[normalize-space()='#501']]",
            ),
        )
        .await?;
        find(
            driver,
            By::Css(".run-session-detail a[href='/projects/demo/automation/runs/501/log']"),
        )
        .await?;
        let list = find(driver, By::Css(".run-session-list")).await?;
        let topbar = find(driver, By::Css(".app-topbar")).await?;
        click_route(
            driver,
            ".run-session-detail .secondary-link",
            "/runs?project=demo&run=502",
        )
        .await?;
        find(
            driver,
            By::Css(".run-session-detail a[href='/projects/demo/automation/runs/502/log']"),
        )
        .await?;
        // Removing the query selection must not retain a mirrored, stale run id.
        click_route(
            driver,
            ".run-session-detail .secondary-link",
            "/runs?project=demo",
        )
        .await?;
        find(
            driver,
            By::Css(".run-session-detail a[href='/projects/demo/automation/runs/503/log']"),
        )
        .await?;
        assert_that!(
            find(driver, By::Css(".run-session-list"))
                .await?
                .element_id()
        )
        .is_equal_to(list.element_id());
        assert_that!(find(driver, By::Css(".app-topbar")).await?.element_id())
            .is_equal_to(topbar.element_id());
        assert_that!(row.text().await?).contains("#501");

        click(driver, By::Css(".thinking-history-toggle")).await?;
        let thinking = find(driver, By::Css(".output-reasoning-history")).await?;
        let response = browser_request(driver, reqwest::Method::POST, "/projects/demo/update")
            .await?
            .form(&[("display_name", "Renamed Demo")])
            .send()
            .await?;
        response_text(response, "project metadata refresh").await?;
        wait_until("updated project switcher", || async {
            let text = find(driver, By::Css(".project-switcher"))
                .await?
                .text()
                .await?;
            Ok(text.contains("Renamed Demo").then_some(()))
        })
        .await?;
        assert_that!(
            find(driver, By::Css(".run-session-list"))
                .await?
                .element_id()
        )
        .is_equal_to(list.element_id());
        assert_that!(
            find(driver, By::Css(".output-reasoning-history"))
                .await?
                .element_id()
        )
        .is_equal_to(thinking.element_id());
        assert_that!(row.text().await?).contains("#501");
        assert_persistent_top_bar(driver).await?;
        Ok(())
    }
}

/// Exercise same-route navigation through a native link even where the product
/// has no direct link between two records. CDP only changes the destination;
/// WebDriver performs the click and Leptos handles the navigation.
async fn click_route(driver: &WebDriver, selector: &str, href: &str) -> Result<(), Report> {
    let document = driver.cdp().send_raw("DOM.getDocument", json!({})).await?;
    let node = driver
        .cdp()
        .send_raw(
            "DOM.querySelector",
            json!({
                "nodeId": document["root"]["nodeId"],
                "selector": selector,
            }),
        )
        .await?;
    driver
        .cdp()
        .send_raw(
            "DOM.setAttributeValue",
            json!({
                "nodeId": node["nodeId"],
                "name": "href",
                "value": href,
            }),
        )
        .await
        .context("failed to prepare native navigation link")?;
    click(driver, By::Css(selector)).await
}

async fn assert_persistent_top_bar(driver: &WebDriver) -> Result<(), Report> {
    let topbar = find(driver, By::Css(".app-topbar")).await?;
    let badge = find(driver, By::Css(".topbar-codex-state")).await?;
    let readiness = badge.text().await?;
    driver
        .cdp()
        .send_raw("Network.setBlockedURLs", json!({"urls": ["*/leptos/*"]}))
        .await?;
    let navigation = async {
        for (path, shell) in [
            ("/knowledge", ".knowledge-page"),
            ("/project", ".project-page"),
            ("/runs", ".runs-page"),
            ("/projects", ".projects-page"),
        ] {
            click(
                driver,
                By::Css(&format!(".top-nav a[href='{path}?project=demo']")),
            )
            .await?;
            find(driver, By::Css(shell)).await?;
            find(
                driver,
                By::Css(&format!(".top-nav a.active[href='{path}?project=demo']")),
            )
            .await?;
            assert_that!(find(driver, By::Css(".app-topbar")).await?.element_id())
                .is_equal_to(topbar.element_id());
            assert_that!(
                find(driver, By::Css(".topbar-codex-state"))
                    .await?
                    .element_id()
            )
            .is_equal_to(badge.element_id());
            assert_that!(badge.text().await?).is_equal_to(readiness.clone());
        }
        Ok::<_, Report>(())
    }
    .await;
    driver
        .cdp()
        .send_raw("Network.setBlockedURLs", json!({"urls": []}))
        .await?;
    navigation
}
