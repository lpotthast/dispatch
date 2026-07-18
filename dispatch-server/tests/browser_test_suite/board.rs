use std::{borrow::Cow, time::Duration};

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

use super::common::*;

pub(crate) struct BoardShellTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for BoardShellTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("board shell and navigation render")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to open Dispatch board page")?;

        find(driver, By::Css("section.board")).await?;
        assert_board_shell_uses_viewport_width(driver).await?;
        assert_board_layout_is_lane_first(driver).await?;
        find(
            driver,
            By::Css(".workspace-dock .workspace-bar > .workspace-actions"),
        )
        .await?;
        assert_that!(
            driver
                .find_all(By::Css(".workspace-dock .workspace-label"))
                .await
                .context("failed to inspect workspace dock labels")?
                .len()
        )
        .is_equal_to(0);
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Dispatch");
        assert_source_contains(driver, "Copy path").await?;
        assert_source_does_not_contain(driver, "Copy cd").await?;
        assert_source_contains(driver, "Open folder").await?;
        assert_source_contains(driver, "Open RustRover").await?;
        assert_source_contains(driver, "Open VS Code").await?;
        find(
            driver,
            By::Css("img.workspace-button-icon[src=\"/icons/workspace-rustrover.svg\"]"),
        )
        .await?;
        find(
            driver,
            By::Css("img.workspace-button-icon[src=\"/icons/workspace-vscode.svg\"]"),
        )
        .await?;
        assert_source_contains(driver, "Git repository").await?;
        find(driver, By::Css(".workspace-git-status")).await?;
        find(driver, By::Css(".workspace-git-diff")).await?;
        assert_source_does_not_contain(driver, "Open IDE").await?;
        assert_source_does_not_contain(driver, "project-settings").await?;
        assert_source_does_not_contain(driver, "System prompt").await?;
        assert_source_does_not_contain(driver, "Memory").await?;
        assert_source_does_not_contain(driver, "Automation policy").await?;
        assert_source_does_not_contain(driver, "Maintenance").await?;
        assert_settings_response_omits_refinement_policy(driver).await?;
        assert_source_does_not_contain(driver, "Run settings").await?;
        assert_top_nav_order(driver).await?;
        find(driver, By::Css(".top-nav a[href='/runs?project=demo']")).await?;
        assert_source_does_not_contain(driver, "No runs yet").await?;
        assert_source_does_not_contain(driver, "CrudKit resources").await?;
        find(driver, By::Css(".topbar-codex")).await?;
        assert_source_does_not_contain(driver, "codex-status-panel").await?;
        find(driver, By::Css(".topbar-auto-commit leptonic-toggle")).await?;
        assert_auto_commit_toggle_updates_without_navigation(driver).await?;
        find(driver, By::Css(".topbar-automation button")).await?;
        assert_source_contains(driver, "Stopped").await?;
        assert_source_does_not_contain(driver, "Start automation").await?;
        assert_source_does_not_contain(driver, "Recover stale claims").await?;
        assert_source_does_not_contain(driver, "Cleanup worktrees").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"work-items\"").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"swim-lanes\"").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"work-item-states\"").await?;
        assert_source_does_not_contain(driver, "Deserialize(").await?;
        assert_source_does_not_contain(driver, "missing field `identifier`").await?;
        assert_source_does_not_contain(driver, "unknown variant `Position`").await?;
        find(driver, By::Css(".lane:nth-child(1) .lane-edit")).await?;
        find(driver, By::Css(".lane:nth-child(1) .lane-header .lane-add")).await?;
        find(driver, By::Css(".lane:nth-child(2) .lane-add")).await?;
        assert_lane_add_button_count(driver, 2).await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"automation-triggers\"")
            .await?;
        assert_frontend_route_navigation_renders_page(driver).await?;
        assert_service_get_results_use_local_storage(driver).await?;

        Ok(())
    }
}

async fn assert_frontend_route_navigation_renders_page(driver: &WebDriver) -> Result<(), Report> {
    click(driver, By::Css(".top-nav a[href='/projects?project=demo']")).await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
    )
    .await?;
    find(driver, By::Css(".workspace-dock")).await?;
    click(driver, By::Css(".top-nav a[href='/?project=demo']")).await?;
    find(driver, By::Css("section.board")).await?;
    find(driver, By::Css(".workspace-dock .workspace-actions")).await?;

    click(driver, By::Css(".top-nav a[href='/projects?project=demo']")).await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='projects'] .crud-nav"),
    )
    .await
    .context("frontend route navigation did not render the Projects page")?;

    click(driver, By::Css(".top-nav a[href='/?project=demo']")).await?;
    find(driver, By::Css("section.board")).await?;
    Ok(())
}

async fn assert_service_get_results_use_local_storage(driver: &WebDriver) -> Result<(), Report> {
    let current_url = driver
        .current_url()
        .await
        .context("failed to read browser URL for local-storage inspection")?;
    let entries = driver
        .cdp()
        .send_raw(
            "DOMStorage.getDOMStorageItems",
            serde_json::json!({
                "storageId": {
                    "securityOrigin": current_url.origin().ascii_serialization(),
                    "isLocalStorage": true
                }
            }),
        )
        .await
        .context("failed to inspect service GET local-storage caches")?;
    let keys = entries["entries"]
        .as_array()
        .context("local-storage CDP response did not contain entries")?
        .iter()
        .filter_map(|entry| entry.as_array()?.first()?.as_str())
        .collect::<Vec<_>>();
    for required in [
        "dispatch.query.board.v2",
        "dispatch.query.board-items.v2",
        "dispatch.query.projects.v1",
        "dispatch.query.workspace-bar.v1",
    ] {
        assert_that!(keys.contains(&required)).is_true();
    }
    Ok(())
}

async fn assert_auto_commit_toggle_updates_without_navigation(
    driver: &WebDriver,
) -> Result<(), Report> {
    let original_url = driver
        .current_url()
        .await
        .context("failed to read URL before Auto-Commit update")?;
    let slider = find(
        driver,
        By::Css(".topbar-auto-commit leptonic-toggle .slider"),
    )
    .await?;
    assert_that!(
        slider
            .class_name()
            .await
            .context("failed to inspect initial Auto-Commit state")?
            .unwrap_or_default()
    )
    .contains("on");

    click(driver, By::Css(".topbar-auto-commit leptonic-toggle")).await?;

    wait_until("Auto-Commit toggle update", || async {
        let slider = find(
            driver,
            By::Css(".topbar-auto-commit leptonic-toggle .slider"),
        )
        .await?;
        let switched_off = !slider
            .class_name()
            .await
            .context("failed to inspect updated Auto-Commit state")?
            .unwrap_or_default()
            .split_whitespace()
            .any(|class| class == "on");
        let response = browser_request(driver, reqwest::Method::GET, "/api/projects/demo/settings")
            .await?
            .send()
            .await
            .context("failed to read persisted Auto-Commit state")?;
        let persisted = response.status().is_success()
            && response
                .json::<serde_json::Value>()
                .await
                .context("failed to decode persisted Auto-Commit state")?["auto_commit"]
                == false;
        let same_url = driver
            .current_url()
            .await
            .context("failed to read URL after Auto-Commit update")?
            == original_url;
        Ok((switched_off && persisted && same_url).then_some(()))
    })
    .await?;

    Ok(())
}

async fn assert_settings_response_omits_refinement_policy(
    driver: &WebDriver,
) -> Result<(), Report> {
    let response = browser_request(driver, reqwest::Method::GET, "/api/projects/demo/settings")
        .await?
        .send()
        .await
        .context("failed to inspect project settings API response")?;
    let status = response.status();
    let settings: serde_json::Value = response
        .json()
        .await
        .context("failed to read project settings API field check")?;
    if !status.is_success() {
        bail!("project settings request failed with {status}: {settings}");
    }
    assert_that!(
        settings
            .get("allow_refinement_agents_during_editing")
            .is_none()
    )
    .is_true();
    Ok(())
}

async fn assert_lane_add_button_count(driver: &WebDriver, expected: usize) -> Result<(), Report> {
    let count = driver
        .find_all(By::Css(".lane .lane-add"))
        .await
        .context("failed to count lane add buttons")?
        .len();
    assert_that!(count).is_equal_to(expected);
    Ok(())
}

async fn assert_board_shell_uses_viewport_width(driver: &WebDriver) -> Result<(), Report> {
    driver
        .set_window_rect(0, 0, 1800, 1000)
        .await
        .context("failed to widen browser test window")?;
    let shell_width = find(driver, By::Css("main.page-shell"))
        .await?
        .rect()
        .await
        .context("failed to inspect board shell width")?
        .width;
    let (viewport_width, _) = layout_viewport_size(driver).await?;
    if shell_width < viewport_width - 1.0 {
        bail!("board shell width {shell_width}px did not fill viewport width {viewport_width}px");
    }
    Ok(())
}

async fn assert_board_layout_is_lane_first(driver: &WebDriver) -> Result<(), Report> {
    let main = find(driver, By::Css("main.page-shell")).await?;
    let workspace = find(driver, By::Css(".workspace-dock")).await?;
    let board = main
        .find(By::Css("section.board"))
        .await
        .context("failed to find compact Board")?;
    let lane_add = board
        .find(By::Css(".lane-add"))
        .await
        .context("failed to find Board lane-add control")?;
    for selector in ["h1", ".board-toolbar", ".runtime-panel", ".workspace-dock"] {
        assert_that!(
            main.find_all(By::Css(selector))
                .await
                .context_with(|| format!("failed to inspect compact Board selector {selector}"))?
                .is_empty()
        )
        .is_true();
    }
    find(
        driver,
        By::Css("main.page-shell > :first-child section.board"),
    )
    .await?;
    assert_that!(
        workspace
            .css_value("position")
            .await
            .context("failed to inspect workspace dock positioning")?
    )
    .is_equal_to("fixed");
    let workspace_rect = workspace
        .rect()
        .await
        .context("failed to inspect workspace dock geometry")?;
    let board_rect = board
        .rect()
        .await
        .context("failed to inspect Board geometry")?;
    let (_, viewport_height) = layout_viewport_size(driver).await?;
    assert_that!((workspace_rect.y + workspace_rect.height - viewport_height).abs() <= 1.0)
        .is_true();
    if workspace_rect.height >= 100.0 {
        bail!(
            "workspace bar was not compact: {}px high",
            workspace_rect.height
        );
    }
    lane_add
        .find(By::XPath("ancestor::*[contains(@class, 'lane-header')][1]"))
        .await
        .context("lane-add control was not in the lane header")?;
    let lanes = board
        .find_all(By::Css(".lane"))
        .await
        .context("failed to inspect Board lanes")?;
    let mut lane_rects = Vec::with_capacity(lanes.len());
    for lane in &lanes {
        lane_rects.push(
            lane.rect()
                .await
                .context("failed to inspect Board lane geometry")?,
        );
    }
    let first_height = lane_rects
        .first()
        .context("Board did not render any lanes")?
        .height;
    assert_that!(
        lane_rects
            .iter()
            .all(|rect| (rect.height - first_height).abs() <= 1.0)
    )
    .is_true();
    let empty_lane = find(
        driver,
        By::XPath(
            "//section[contains(@class, 'lane')][.//*[contains(@class, 'lane-count')][normalize-space()='0']]",
        ),
    )
    .await?;
    let empty_lane_rect = empty_lane
        .rect()
        .await
        .context("failed to inspect empty Board lane geometry")?;
    assert_that!(
        board_rect.y + board_rect.height - empty_lane_rect.y - empty_lane_rect.height <= 12.0
    )
    .is_true();
    assert_that!(
        lane_rects
            .iter()
            .all(|rect| { rect.y + rect.height <= workspace_rect.y + 1.0 })
    )
    .is_true();
    assert_that!(workspace_rect.y - board_rect.y - board_rect.height <= 32.0).is_true();

    driver
        .set_window_rect(0, 0, 390, 900)
        .await
        .context("failed to resize browser for narrow Board layout")?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let lane_add = find(driver, By::Css(".lane .lane-add")).await?;
    assert_that!(
        lane_add
            .css_value("opacity")
            .await
            .context("failed to inspect narrow Board lane-add opacity")?
    )
    .is_equal_to("1");
    assert_that!(
        lane_add
            .css_value("pointer-events")
            .await
            .context("failed to inspect narrow Board lane-add pointer events")?
    )
    .is_equal_to("auto");
    lane_add
        .find(By::XPath("ancestor::*[contains(@class, 'lane-header')][1]"))
        .await
        .context("narrow Board lane-add control was not in its header")?;
    let lane_rect = find(driver, By::Css(".lane"))
        .await?
        .rect()
        .await
        .context("failed to inspect narrow Board lane geometry")?;
    let board_rect = find(driver, By::Css("section.board"))
        .await?
        .rect()
        .await
        .context("failed to inspect narrow Board geometry")?;
    let workspace_rect = find(driver, By::Css(".workspace-dock"))
        .await?
        .rect()
        .await
        .context("failed to inspect narrow workspace dock geometry")?;
    assert_that!(lane_rect.y + lane_rect.height <= workspace_rect.y + 1.0).is_true();
    assert_that!(workspace_rect.y - board_rect.y - board_rect.height <= 32.0).is_true();

    driver
        .set_window_rect(0, 0, 1800, 1000)
        .await
        .context("failed to restore desktop browser size")?;
    Ok(())
}

async fn assert_top_nav_order(driver: &WebDriver) -> Result<(), Report> {
    let links = driver
        .find_all(By::Css(".top-nav a"))
        .await
        .context("failed to inspect top navigation")?;
    let mut label_parts = Vec::with_capacity(links.len());
    for link in links {
        label_parts.push(
            link.text()
                .await
                .context("failed to read top navigation label")?,
        );
    }
    let labels = label_parts.join("|");
    assert_that!(labels)
        .is_equal_to("Board|Project|Automation|Runs|Projects|System|API".to_owned());
    Ok(())
}
