use std::{borrow::Cow, time::Duration};

use assertr::prelude::*;
use browser_test::thirtyfour::{
    By, WebDriver,
    cdp::domains::input::{DispatchMouseEvent, MouseButton, MouseEventType},
};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

use super::common::*;

pub(crate) struct BoardDrawerTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for BoardDrawerTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("board drawer preserves navigation and editing")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        seed_grouped_work_items(driver).await?;
        seed_run_commit_outcome_fixtures(app).await?;
        create_labeled_browser_test_item(driver).await?;
        link_run_fixtures_to_browser_item(app).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to open Board drawer fixture")?;
        driver
            .set_window_rect(0, 0, 1800, 1000)
            .await
            .context("failed to set desktop Board drawer viewport")?;
        find_browser_item_card_link(driver).await?;
        assert_board_drawer_navigation(driver, app).await?;

        open_browser_item_drawer(driver).await?;
        find(driver, By::Css("section.item-settings")).await?;
        find(driver, By::Css("section.comments")).await?;
        assert_source_contains(driver, "Item details").await?;
        assert_source_contains(driver, "area=browser").await?;
        assert_source_contains(driver, "needs-verification").await?;
        assert_item_drawer_detail_layout(driver).await?;
        assert_item_detail_description_is_not_duplicated(driver).await?;
        assert_item_detail_description_editor_accepts_click_and_text(driver).await?;
        let relationship_target_id = create_relationship_target_item(driver).await?;
        assert_item_relationship_create_delete_flow(driver, relationship_target_id).await?;
        assert_board_drawer_dirty_leave_protection(driver).await?;
        assert_board_drawer_delete_flow(driver).await?;

        Ok(())
    }
}

async fn create_relationship_target_item(driver: &WebDriver) -> Result<i64, Report> {
    create_browser_test_item(
        driver,
        "Relationship target",
        "Created as a browser-test relationship target",
    )
    .await
}

async fn find_browser_item_card_link(
    driver: &WebDriver,
) -> Result<browser_test::thirtyfour::WebElement, Report> {
    find(
        driver,
        By::XPath(
            "//article[contains(@class, 'card')]//a[contains(@class, 'card-main-link')][.//h3[normalize-space()='Browser item']]",
        ),
    )
    .await
}

async fn open_browser_item_drawer(driver: &WebDriver) -> Result<(), Report> {
    let link = find_browser_item_card_link(driver).await?;
    click_element(&link, "open Browser item drawer").await?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    Ok(())
}

async fn assert_item_drawer_detail_layout(driver: &WebDriver) -> Result<(), Report> {
    let content = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown .item-detail-content"),
    )
    .await?;
    let labels = content
        .find(By::Css("section.item-labels"))
        .await
        .context("failed to find drawer item labels")?;
    let children = content
        .find_all(By::Css(":scope > *"))
        .await
        .context("failed to inspect drawer item-detail sections")?;
    let mut previous_bottom = None;
    for child in children {
        let rect = child
            .rect()
            .await
            .context("failed to inspect drawer item-detail section geometry")?;
        if let Some(bottom) = previous_bottom {
            assert_that!(rect.y - bottom >= 15.0).is_true();
        }
        previous_bottom = Some(rect.y + rect.height);
    }
    assert_that!(
        labels
            .find_all(By::Css(".label-chip"))
            .await
            .context("failed to inspect drawer label chips")?
            .len()
    )
    .is_equal_to(0);
    let area = labels
        .find(By::Css(".label-row[data-label-key='area']"))
        .await
        .context("failed to find drawer area label")?;
    assert_that!(
        element_value(
            &area
                .find(By::Css("input[name='key']"))
                .await
                .context("failed to find drawer area-label key")?,
            "drawer area-label key",
        )
        .await?
    )
    .is_equal_to("area");
    assert_that!(
        element_value(
            &area
                .find(By::Css("input[name='value']"))
                .await
                .context("failed to find drawer area-label value")?,
            "drawer area-label value",
        )
        .await?
    )
    .is_equal_to("browser");
    let state = labels
        .find(By::Css(".label-row[data-label-key='state']"))
        .await
        .context("failed to find drawer state label")?;
    assert_that!(
        state
            .find(By::Css(".label-fixed-key strong"))
            .await
            .context("failed to find drawer state-label key")?
            .text()
            .await
            .context("failed to read drawer state-label key")?
    )
    .is_equal_to("State");
    assert_that!(
        state
            .find_all(By::Css("input[name='key']"))
            .await
            .context("failed to inspect drawer state-label key input")?
            .is_empty()
    )
    .is_true();
    for row in labels
        .find_all(By::Css(".label-row"))
        .await
        .context("failed to inspect drawer label rows")?
    {
        let scroll_width = element_numeric_property(&row, "scrollWidth", "label-row width").await?;
        let client_width = element_numeric_property(&row, "clientWidth", "label-row width").await?;
        assert_that!(scroll_width <= client_width + 1.0).is_true();
    }
    let buttons = labels
        .find_all(By::Css(".label-row-actions button"))
        .await
        .context("failed to inspect drawer label actions")?;
    let mut button_rects = Vec::with_capacity(buttons.len());
    for button in buttons {
        button_rects.push(
            button
                .rect()
                .await
                .context("failed to inspect drawer label-action geometry")?,
        );
    }
    for (index, left) in button_rects.iter().enumerate() {
        for right in &button_rects[index + 1..] {
            assert_that!(!rects_overlap(left, right)).is_true();
        }
    }
    Ok(())
}

async fn assert_board_drawer_navigation(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    let item_link = find_browser_item_card_link(driver).await?;
    let Some(item_id) = item_link
        .attr("data-board-item-id")
        .await
        .context("failed to read Browser item card id")?
    else {
        bail!("Browser item card did not expose its item id");
    };
    let item_id = item_id
        .parse::<i64>()
        .context("failed to parse Browser item card id")?;

    for card in driver
        .find_all(By::Css("article.card"))
        .await
        .context("failed to inspect visible Board item ids")?
    {
        let item_id = card
            .find(By::Css(".card-main-link"))
            .await
            .context("failed to find Board card link")?
            .attr("data-board-item-id")
            .await
            .context("failed to read Board card item id")?
            .context("Board card link had no item id")?;
        let footer = card
            .find(By::Css("footer"))
            .await
            .context("failed to find Board card footer")?;
        let visible_id = footer
            .find(By::Css(":scope > .card-item-id:first-child"))
            .await
            .context("Board item id was not the first footer child")?;
        assert_that!(
            visible_id
                .text()
                .await
                .context("failed to read visible Board item id")?
        )
        .is_equal_to(format!("#{item_id}"));
        let item_id_color = visible_id
            .css_value("color")
            .await
            .context("failed to read visible Board item-id color")?;
        let footer_color = footer
            .css_value("color")
            .await
            .context("failed to read Board card-footer color")?;
        assert_that!(item_id_color).is_equal_to(footer_color);
        for child in footer
            .find_all(By::Css(":scope > *"))
            .await
            .context("failed to inspect Board card footer children")?
        {
            let text = child
                .text()
                .await
                .context("failed to read Board card footer child")?;
            assert_that!(is_version_badge(&text)).is_false();
        }
    }

    let previews = wait_until("Board run previews", || async {
        let cards = driver
            .find_all(By::XPath(
                "//article[contains(@class, 'card')][.//*[contains(@class, 'card-main-link')]//h3[normalize-space()='Browser item']]",
            ))
            .await
            .context("failed to inspect Browser item cards")?;
        for card in cards {
            let Some(count_element) = card
                .find_all(By::Css(".card-run-count"))
                .await
                .context("failed to inspect Board run count")?
                .into_iter()
                .next()
            else {
                continue;
            };
            let count = count_element
                .text()
                .await
                .context("failed to read Board run count")?;
            if count != "RUNS 3" {
                continue;
            }
            let links = card
                .find_all(By::Css(".card-run-preview"))
                .await
                .context("failed to inspect Board run previews")?;
            let mut runs = Vec::with_capacity(links.len());
            for link in links {
                runs.push(format!(
                    "{}|{}",
                    link.attr("href")
                        .await
                        .context("failed to read Board run-preview href")?
                        .unwrap_or_default(),
                    link.class_name()
                        .await
                        .context("failed to read Board run-preview classes")?
                        .unwrap_or_default(),
                ));
            }
            return Ok(Some(format!("{count};{}", runs.join(";"))));
        }
        Ok(None)
    })
    .await;
    let previews = match previews {
        Ok(previews) => previews,
        Err(error) => {
            let cards = driver
                .find_all(By::XPath(
                    "//article[contains(@class, 'card')][.//*[contains(@class, 'card-main-link')]//h3[normalize-space()='Browser item']]",
                ))
                .await
                .context("failed to diagnose Browser item run previews")?;
            let mut summaries = Vec::with_capacity(cards.len());
            for card in cards {
                summaries.push(
                    card.text()
                        .await
                        .context("failed to read Browser item card diagnostics")?,
                );
            }
            bail!("{error}; Browser item cards: {summaries:?}");
        }
    };
    assert_that!(previews).is_equal_to(
        "RUNS 3;/projects/demo/automation/runs/503/log|card-run-preview status-completed;/projects/demo/automation/runs/502/log|card-run-preview status-failed;/projects/demo/automation/runs/501/log|card-run-preview status-completed"
            .to_owned(),
    );

    assert_middle_click_opens_canonical_item(driver, item_id).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read Board URL after middle-click")?
            .path()
    )
    .is_equal_to("/");

    let closed_lane_width = inspect_board_lane_width(driver).await?;
    open_browser_item_drawer(driver).await?;
    assert_drawer_url(driver, item_id, None).await?;
    find(driver, By::Css("section.board")).await?;
    let desktop = inspect_drawer_layout(driver).await?;
    if !desktop.starts_with("modal=false;backdrop=none;") {
        bail!("desktop Board drawer was unexpectedly modal: {desktop}");
    }
    assert_desktop_drawer_preserves_board_access(driver).await?;
    assert_board_lane_width(driver, closed_lane_width).await?;
    assert_board_drawer_is_resizable(driver).await?;

    let direct_url = driver
        .current_url()
        .await
        .context("failed to read direct drawer URL")?;
    driver
        .goto(direct_url.as_str())
        .await
        .context("failed to reload direct item drawer URL")?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    find(driver, By::Css("section.board")).await?;

    driver
        .back()
        .await
        .context("failed to close drawer with Back")?;
    wait_for_drawer_visibility(driver, false).await?;
    find(driver, By::Css("section.board")).await?;
    driver
        .forward()
        .await
        .context("failed to restore drawer with Forward")?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    assert_drawer_url(driver, item_id, None).await?;

    click_board_run_preview(driver, 503).await?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown .run-drawer-header"),
    )
    .await?;
    assert_drawer_url(driver, item_id, Some(503)).await?;
    assert_source_contains(driver, "Developer instructions").await?;
    assert_source_contains(driver, "User prompt").await?;
    find(
        driver,
        By::Css(".run-drawer-header a[href='/projects/demo/automation/runs/503/log']"),
    )
    .await?;

    click(driver, By::Css(".board-drawer-back")).await?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    assert_drawer_url(driver, item_id, None).await?;

    click_board_run_preview(driver, 503).await?;
    find(driver, By::Css(".run-drawer-header")).await?;
    click(
        driver,
        By::Css(".run-drawer-header a[href='/projects/demo/automation/runs/503/log']"),
    )
    .await?;
    find(driver, By::Css("main.page-shell.run-log")).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read canonical run URL")?
            .path()
    )
    .is_equal_to("/projects/demo/automation/runs/503/log");

    driver
        .goto(app.url("/?project=demo"))
        .await
        .context("failed to return to Board after full run navigation")?;
    open_browser_item_drawer(driver).await?;
    click(driver, By::Css(".board-drawer-title")).await?;
    find(
        driver,
        By::Css("main.page-shell.item-page section.item-settings"),
    )
    .await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read canonical item URL")?
            .path()
    )
    .is_equal_to(format!("/projects/demo/items/{item_id}"));

    driver
        .goto(app.url("/?project=demo"))
        .await
        .context("failed to return to Board after full item navigation")?;
    open_browser_item_drawer(driver).await?;
    click_board_card_by_title(driver, "Grouped finding one").await?;
    find(
        driver,
        By::XPath(
            "//leptonic-drawer[contains(@class, 'shown')]//a[contains(@class, 'board-drawer-title')]//h1[contains(., 'Grouped finding one')]",
        ),
    )
    .await?;
    click_board_card_by_title(driver, "Browser item").await?;
    find(
        driver,
        By::XPath(
            "//leptonic-drawer[contains(@class, 'shown')]//a[contains(@class, 'board-drawer-title')]//h1[contains(., 'Browser item')]",
        ),
    )
    .await?;

    driver
        .set_window_rect(0, 0, 390, 900)
        .await
        .context("failed to resize browser for narrow drawer")?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let narrow = inspect_drawer_layout(driver).await?;
    if !narrow.starts_with("modal=true;backdrop=auto;") {
        bail!("narrow Board drawer did not become modal: {narrow}");
    }
    let Some((drawer_width, viewport_width)) = narrow
        .split(';')
        .find_map(|part| part.strip_prefix("width=")?.split_once('/'))
    else {
        bail!("failed to parse narrow drawer width: {narrow}");
    };
    let drawer_width = drawer_width
        .parse::<i64>()
        .context("failed to parse narrow drawer width")?;
    let viewport_width = viewport_width
        .parse::<i64>()
        .context("failed to parse narrow viewport width")?;
    if (drawer_width - viewport_width).abs() > 1 {
        bail!("narrow drawer width {drawer_width}px did not match viewport {viewport_width}px");
    }
    assert_drawer_starts_below_topbar(driver).await?;
    driver
        .set_window_rect(0, 0, 1800, 1000)
        .await
        .context("failed to restore desktop browser size after drawer test")?;

    click(driver, By::Css(".board-drawer-close")).await?;
    wait_for_drawer_visibility(driver, false).await?;

    driver
        .goto(app.url("/?project=demo&item=999999999"))
        .await
        .context("failed to open invalid direct drawer URL")?;
    find(
        driver,
        By::XPath("//leptonic-toast[@data-variant='error'][contains(., 'Drawer unavailable')]"),
    )
    .await?;
    wait_for_drawer_visibility(driver, false).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read cleared invalid drawer URL")?
            .query()
    )
    .is_equal_to(Some("project=demo"));
    Ok(())
}

async fn assert_middle_click_opens_canonical_item(
    driver: &WebDriver,
    item_id: i64,
) -> Result<(), Report> {
    let original_window = driver
        .window()
        .await
        .context("failed to read original Board window")?;
    let original_windows = driver
        .windows()
        .await
        .context("failed to list original Board windows")?;
    let link = find_browser_item_card_link(driver).await?;
    link.scroll_into_view()
        .await
        .context("failed to scroll Browser item card into view")?;
    let rect = link
        .rect()
        .await
        .context("failed to locate Browser item for middle-click")?;
    let (page_x, page_y) = layout_viewport_offset(driver).await?;
    let x = rect.x - page_x + rect.width / 2.0;
    let y = rect.y - page_y + (rect.height / 2.0).min(18.0);
    for event_type in [MouseEventType::MousePressed, MouseEventType::MouseReleased] {
        driver
            .cdp()
            .send(DispatchMouseEvent {
                r#type: event_type,
                x,
                y,
                modifiers: None,
                button: Some(MouseButton::Middle),
                click_count: Some(1),
                delta_x: None,
                delta_y: None,
            })
            .await
            .context("failed to dispatch native middle-click")?;
    }

    let new_window = {
        let mut found = None;
        for _ in 0..20 {
            let windows = driver
                .windows()
                .await
                .context("failed to list windows after middle-click")?;
            found = windows
                .into_iter()
                .find(|window| !original_windows.contains(window));
            if found.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let Some(found) = found else {
            bail!("middle-click did not open a new item tab");
        };
        found
    };
    driver
        .switch_to_window(new_window)
        .await
        .context("failed to switch to middle-click item tab")?;
    find(
        driver,
        By::Css("main.page-shell.item-page section.item-settings"),
    )
    .await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read middle-click item URL")?
            .path()
    )
    .is_equal_to(format!("/projects/demo/items/{item_id}"));
    driver
        .close_window()
        .await
        .context("failed to close middle-click item tab")?;
    driver
        .switch_to_window(original_window)
        .await
        .context("failed to return to original Board tab")?;
    Ok(())
}

async fn assert_board_drawer_dirty_leave_protection(driver: &WebDriver) -> Result<(), Report> {
    let Some(item_id) = find_browser_item_card_link(driver)
        .await?
        .attr("data-board-item-id")
        .await
        .context("failed to read dirty drawer item id")?
    else {
        bail!("dirty drawer item link did not expose an item id");
    };
    let item_id = item_id
        .parse::<i64>()
        .context("failed to parse dirty drawer item id")?;
    set_input_value(
        driver,
        "leptonic-drawer.shown section.item-settings .crud-input-field",
        "Unsaved drawer title",
    )
    .await?;

    click(driver, By::Css(".board-drawer-close")).await?;
    find_leave_modal(driver)
        .await
        .context("drawer close did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_dirty_drawer_preserved(driver, item_id).await?;

    click_board_card_by_title(driver, "Grouped finding one").await?;
    find_leave_modal(driver)
        .await
        .context("drawer item switch did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_dirty_drawer_preserved(driver, item_id).await?;

    click_board_run_preview(driver, 503).await?;
    find_leave_modal(driver)
        .await
        .context("drawer run switch did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_dirty_drawer_preserved(driver, item_id).await?;

    click(driver, By::Css(".board-drawer-title")).await?;
    find_leave_modal(driver)
        .await
        .context("drawer full-page action did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_dirty_drawer_preserved(driver, item_id).await?;

    driver
        .back()
        .await
        .context("failed to request browser Back from dirty drawer")?;
    find_leave_modal(driver)
        .await
        .context("browser Back did not open the drawer dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_dirty_drawer_preserved(driver, item_id).await?;

    click(driver, By::Css(".board-drawer-close")).await?;
    find_leave_modal(driver)
        .await
        .context("second drawer close did not open the dirty-leave guard")?;
    click_leave_modal_accept(driver).await?;
    wait_for_drawer_visibility(driver, false).await?;
    find(driver, By::Css("section.board")).await?;
    Ok(())
}

async fn assert_board_drawer_delete_flow(driver: &WebDriver) -> Result<(), Report> {
    let item_id = create_browser_test_item(
        driver,
        "Drawer delete item",
        "Deleted from the Board drawer by browser-test",
    )
    .await?;
    click_board_card_by_title(driver, "Drawer delete item").await?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    click(
        driver,
        By::XPath(
            "//leptonic-drawer[contains(@class, 'shown')]//section[contains(@class, 'item-settings')]//button[contains(., 'Löschen')]",
        ),
    )
    .await?;
    find(
        driver,
        By::XPath("//leptonic-modal[contains(., 'Bist du dir sicher?')]"),
    )
    .await?;
    click(
        driver,
        By::XPath(
            "//leptonic-modal[contains(., 'Bist du dir sicher?')]//leptonic-modal-footer//button[normalize-space()='Löschen']",
        ),
    )
    .await?;

    wait_until("Board drawer deletion result", || async {
        let mut success = false;
        for toast in driver
            .find_all(By::Css("leptonic-toast"))
            .await
            .context("failed to inspect Board drawer deletion toasts")?
        {
            let text = toast
                .text()
                .await
                .context("failed to read Board drawer deletion toast")?;
            if text.contains(&format!("item {item_id} does not exist in this project")) {
                bail!("stale item error toast: {text}");
            }
            success |= text.contains("erfolgreich gelöscht");
        }
        let drawer_closed = driver
            .find_all(By::Css("leptonic-drawer[data-board-inspector].shown"))
            .await
            .context("failed to inspect drawer after item deletion")?
            .is_empty();
        let card_deleted = driver
            .find_all(By::XPath(
                "//article[contains(@class, 'card')][.//h3[normalize-space()='Drawer delete item']]",
            ))
            .await
            .context("failed to inspect deleted Board card")?
            .is_empty();
        let query = driver
            .current_url()
            .await
            .context("failed to read URL after drawer item deletion")?
            .query()
            .map(str::to_owned);
        Ok((success && drawer_closed && card_deleted && query.as_deref() == Some("project=demo"))
            .then_some(()))
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(750)).await;
    Ok(())
}

async fn assert_dirty_drawer_preserved(driver: &WebDriver, item_id: i64) -> Result<(), Report> {
    assert_drawer_url(driver, item_id, None).await?;
    let value = element_value(
        &editable_element(
            &find(
                driver,
                By::Css("leptonic-drawer.shown section.item-settings .crud-input-field"),
            )
            .await?,
        )
        .await?,
        "dirty drawer title",
    )
    .await?;
    assert_that!(value).is_equal_to("Unsaved drawer title".to_owned());
    Ok(())
}

async fn click_board_card_by_title(driver: &WebDriver, title: &str) -> Result<(), Report> {
    let mut last_error = None;
    for _ in 0..3 {
        let links = driver
            .find_all(By::Css(".card-main-link"))
            .await
            .context("failed to find Board card links")?;
        for link in links {
            let Ok(heading) = link.find(By::Css("h3")).await else {
                continue;
            };
            let Ok(found_title) = heading.text().await else {
                continue;
            };
            if found_title.trim() != title {
                continue;
            }

            if let Err(error) = link.scroll_into_view().await {
                last_error = Some(error.to_string());
                continue;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            match link.click().await {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    bail!(
        "failed to click Board card {title:?} through the uncovered Board: {}",
        last_error.unwrap_or_else(|| "matching card was not found".to_owned())
    )
}

async fn click_board_run_preview(driver: &WebDriver, run_id: i64) -> Result<(), Report> {
    click(
        driver,
        By::XPath(format!(
            "//article[contains(@class, 'card')][.//h3[normalize-space()='Browser item']]//a[contains(@class, 'card-run-preview')][substring(@href, string-length(@href) - string-length('/runs/{run_id}/log') + 1) = '/runs/{run_id}/log']"
        )),
    )
    .await
    .context("failed to click Board run preview")?;
    Ok(())
}

async fn assert_drawer_url(
    driver: &WebDriver,
    item_id: i64,
    run_id: Option<i64>,
) -> Result<(), Report> {
    let expected = match run_id {
        Some(run_id) => format!("project=demo&item={item_id}&run={run_id}"),
        None => format!("project=demo&item={item_id}"),
    };
    for _ in 0..30 {
        let query = driver
            .current_url()
            .await
            .context("failed to read Board drawer URL")?
            .query()
            .unwrap_or_default()
            .to_owned();
        if query == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let current = driver
        .current_url()
        .await
        .context("failed to read final Board drawer URL")?;
    bail!("expected Board drawer query {expected:?}, got {current}");
}

async fn wait_for_drawer_visibility(driver: &WebDriver, expected: bool) -> Result<(), Report> {
    wait_until("Board drawer visibility", || async {
        let shown = !driver
            .find_all(By::Css("leptonic-drawer[data-board-inspector].shown"))
            .await
            .context("failed to inspect Board drawer visibility")?
            .is_empty();
        Ok((shown == expected).then_some(()))
    })
    .await?;
    Ok(())
}

async fn inspect_drawer_layout(driver: &WebDriver) -> Result<String, Report> {
    let drawer = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown"),
    )
    .await?;
    let backdrop = find(driver, By::Css(".board-drawer-backdrop.shown")).await?;
    let rect = drawer
        .rect()
        .await
        .context("failed to inspect Board drawer geometry")?;
    let (viewport_width, _) = layout_viewport_size(driver).await?;
    let body = drawer
        .find(By::Css(".board-drawer-body"))
        .await
        .context("failed to find Board drawer body")?;
    Ok(format!(
        "modal={};backdrop={};width={}/{};right={}/{};overflow={}",
        drawer
            .attr("aria-modal")
            .await
            .context("failed to inspect Board drawer modality")?
            .unwrap_or_default(),
        backdrop
            .css_value("pointer-events")
            .await
            .context("failed to inspect Board drawer backdrop")?,
        rect.width.round() as i64,
        viewport_width.round() as i64,
        (rect.x + rect.width).round() as i64,
        viewport_width.round() as i64,
        body.css_value("overflow-y")
            .await
            .context("failed to inspect Board drawer overflow")?,
    ))
}

async fn inspect_board_lane_width(driver: &WebDriver) -> Result<i64, Report> {
    Ok(find(driver, By::Css("section.board > .lane"))
        .await?
        .rect()
        .await
        .context("failed to inspect Board lane width")?
        .width
        .round() as i64)
}

async fn assert_board_lane_width(driver: &WebDriver, expected: i64) -> Result<(), Report> {
    let actual = inspect_board_lane_width(driver).await?;
    if (actual - expected).abs() > 1 {
        bail!("Board lane width changed from {expected}px to {actual}px when the drawer opened");
    }
    Ok(())
}

async fn assert_board_drawer_is_resizable(driver: &WebDriver) -> Result<(), Report> {
    let handle = find(driver, By::Css(".board-drawer-resize-handle")).await?;
    driver
        .action_chain()
        .move_to_element_center(&handle)
        .perform()
        .await
        .context("failed to hover the Board drawer resize handle")?;
    tokio::time::sleep(Duration::from_millis(160)).await;
    assert_that!(
        handle
            .css_value("cursor")
            .await
            .context("failed to inspect the Board drawer resize affordance")?
    )
    .is_equal_to("col-resize");

    let selected = drag_drawer_to_percent(driver, &handle, 60.0).await?;
    let maximum = drag_drawer_to_percent(driver, &handle, 90.0).await?;
    let minimum = drag_drawer_to_percent(driver, &handle, 10.0).await?;
    assert_that!(selected).is_equal_to(60);
    assert_that!(maximum).is_equal_to(70);
    assert_that!(minimum).is_equal_to(30);
    assert_that!(
        handle
            .attr("aria-valuemin")
            .await
            .context("failed to read drawer minimum")?
            .as_deref()
    )
    .is_equal_to(Some("30"));
    assert_that!(
        handle
            .attr("aria-valuemax")
            .await
            .context("failed to read drawer maximum")?
            .as_deref()
    )
    .is_equal_to(Some("70"));
    assert_that!(
        handle
            .attr("aria-valuenow")
            .await
            .context("failed to read drawer current size")?
            .as_deref()
    )
    .is_equal_to(Some("30"));
    Ok(())
}

async fn assert_desktop_drawer_preserves_board_access(driver: &WebDriver) -> Result<(), Report> {
    let topbar = find(driver, By::Css(".app-topbar")).await?;
    let board = find(driver, By::Css("section.board")).await?;
    let drawer = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown"),
    )
    .await?;
    let lanes = board
        .find_all(By::Css(":scope > .lane"))
        .await
        .context("failed to inspect Board lanes beside drawer")?;
    let last_lane = lanes.last().context("Board did not render any lanes")?;
    last_lane
        .scroll_into_view()
        .await
        .context("failed to scroll the last Board lane into view")?;
    let topbar_rect = topbar
        .rect()
        .await
        .context("failed to inspect topbar geometry beside drawer")?;
    let board_rect = board
        .rect()
        .await
        .context("failed to inspect Board geometry beside drawer")?;
    let drawer_rect = drawer
        .rect()
        .await
        .context("failed to inspect drawer geometry beside Board")?;
    let last_lane_rect = last_lane
        .rect()
        .await
        .context("failed to inspect last Board lane geometry")?;
    assert_that!(topbar_rect.y >= -1.0 && topbar_rect.y + topbar_rect.height > 0.0).is_true();
    assert_that!(drawer_rect.y >= topbar_rect.y + topbar_rect.height - 1.0).is_true();
    assert_that!(board_rect.x + board_rect.width <= drawer_rect.x + 1.0).is_true();
    assert_that!(
        board
            .css_value("overflow-x")
            .await
            .context("failed to inspect Board horizontal overflow")?
    )
    .is_equal_to("auto");
    assert_that!(
        element_numeric_property(&board, "scrollWidth", "Board horizontal extent").await?
            > element_numeric_property(&board, "clientWidth", "Board viewport width").await? + 1.0
    )
    .is_true();
    assert_that!(last_lane_rect.x >= board_rect.x - 1.0).is_true();
    assert_that!(last_lane_rect.x + last_lane_rect.width <= board_rect.x + board_rect.width + 1.0)
        .is_true();
    let (_, viewport_height) = layout_viewport_size(driver).await?;
    assert_that!(drawer_rect.height < viewport_height - 1.0).is_true();
    Ok(())
}

async fn assert_drawer_starts_below_topbar(driver: &WebDriver) -> Result<(), Report> {
    let topbar = find(driver, By::Css(".app-topbar"))
        .await?
        .rect()
        .await
        .context("failed to inspect the topbar beside the narrow drawer")?;
    let drawer = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown"),
    )
    .await?
    .rect()
    .await
    .context("failed to inspect the narrow drawer geometry")?;
    assert_that!(topbar.y >= -1.0 && topbar.y + topbar.height > 0.0).is_true();
    assert_that!(drawer.y >= topbar.y + topbar.height - 1.0).is_true();
    Ok(())
}

async fn assert_item_detail_description_is_not_duplicated(
    driver: &WebDriver,
) -> Result<(), Report> {
    let header = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown .board-drawer-header"),
    )
    .await?
    .text()
    .await
    .context("failed to read item header")?;
    if header.contains("Created through browser-test\nSecond line") {
        bail!("item header duplicated the editable description");
    }
    wait_until("item description editor value", || async {
        let value = element_value(
            &find(
                driver,
                By::Css("section.item-settings input[name='description']"),
            )
            .await?,
            "item description",
        )
        .await?;
        Ok(
            (value.contains("Created through browser-test") && value.contains("Second line"))
                .then_some(()),
        )
    })
    .await?;
    find(
        driver,
        By::Css(
            "section.item-settings [data-rich-text-field='description'] leptonic-tiptap-editor",
        ),
    )
    .await?;
    Ok(())
}

async fn assert_item_detail_description_editor_accepts_click_and_text(
    driver: &WebDriver,
) -> Result<(), Report> {
    let original_url = driver
        .current_url()
        .await
        .context("failed to read URL before description editor click")?;

    click_description_editor(driver).await?;
    tokio::time::sleep(Duration::from_millis(250)).await;

    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read URL after description editor click")?
    )
    .is_equal_to(original_url);

    let editor = find(
        driver,
        By::Css("section.item-settings [data-rich-text-field='description'] .ProseMirror"),
    )
    .await?;
    editor
        .send_keys(" Editable after click")
        .await
        .context("failed to type in description editor after click")?;

    let value = wait_until("edited description value", || async {
        let value = element_value(
            &find(
                driver,
                By::Css("section.item-settings input[name='description']"),
            )
            .await?,
            "edited description",
        )
        .await?;
        Ok(value.contains("Editable after click").then_some(value))
    })
    .await?;
    assert_that!(value).contains("Editable after click");
    Ok(())
}

async fn click_description_editor(driver: &WebDriver) -> Result<(), Report> {
    let selector = "section.item-settings [data-rich-text-field='description'] .ProseMirror";
    let mut last_error = None;
    for _ in 0..5 {
        let editor = find(driver, By::Css(selector)).await?;
        match editor.scroll_into_view().await {
            Ok(()) => match editor.click().await {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(format!("click failed: {err}")),
            },
            Err(err) => last_error = Some(format!("scroll failed: {err}")),
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    bail!(
        "failed to click description editor after retries: {}",
        last_error.unwrap_or_else(|| "no click attempt was made".to_owned())
    )
}

async fn assert_item_relationship_create_delete_flow(
    driver: &WebDriver,
    target_id: i64,
) -> Result<(), Report> {
    find(driver, By::Css("section.item-relationships")).await?;
    assert_source_contains(driver, "No relationships").await?;
    let original_url = driver
        .current_url()
        .await
        .context("failed to read item URL before relationship mutation")?;
    replace_element_value(
        &find(
            driver,
            By::Css(".relationship-add-controls input[name='target_work_item_id']"),
        )
        .await?,
        &target_id.to_string(),
        "relationship target id",
    )
    .await?;
    replace_element_value(
        &find(
            driver,
            By::Css(".relationship-add-controls input[name='kind']"),
        )
        .await?,
        "is follow-up of",
        "relationship kind",
    )
    .await?;
    click(driver, By::Css(".relationship-add-controls button")).await?;

    let panel = find(driver, By::Css("section.item-relationships")).await?;
    find(
        driver,
        By::Css(format!(
            ".relationship-related[href='/projects/demo/items/{target_id}']"
        )),
    )
    .await?;
    let panel_text = panel
        .text()
        .await
        .context("failed to read relationship panel after add")?;
    for expected in ["is follow-up of", "outgoing", "Relationship target"] {
        assert_that!(panel_text.clone()).contains(expected);
    }
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read item URL after relationship add")?
    )
    .is_equal_to(original_url.clone());

    replace_element_value(
        &find(
            driver,
            By::Css(".relationship-kind-controls input[name='kind']"),
        )
        .await?,
        "depends on",
        "updated relationship kind",
    )
    .await?;
    click(driver, By::Css(".relationship-kind-controls button")).await?;
    find(driver, By::XPath("//*[contains(text(), 'depends on')]")).await?;

    click(driver, By::Css(".relationship-delete-controls button")).await?;
    wait_until("relationship deletion", || async {
        let panel = find(driver, By::Css("section.item-relationships")).await?;
        let has_rows = !panel
            .find_all(By::Css(".relationship-row"))
            .await
            .context("failed to inspect relationship rows after delete")?
            .is_empty();
        let text = panel
            .text()
            .await
            .context("failed to read relationship panel after delete")?;
        Ok((!has_rows && text.contains("No relationships")).then_some(()))
    })
    .await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read item URL after relationship delete")?
    )
    .is_equal_to(original_url);
    Ok(())
}

fn is_version_badge(value: &str) -> bool {
    value
        .strip_prefix('v')
        .is_some_and(|version| !version.is_empty() && version.chars().all(|ch| ch.is_ascii_digit()))
}
