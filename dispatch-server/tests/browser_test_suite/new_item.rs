use std::{borrow::Cow, time::Duration};

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};

use super::common::*;

pub(crate) struct NewItemWorkflowTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for NewItemWorkflowTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("new item modal creates labeled work")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to open Board for new-item workflow")?;
        open_new_item_modal(driver).await?;
        assert_new_item_modal_actions(driver).await?;
        find(driver, By::Css("#new-item-modal select[name='state']")).await?;
        assert_lane_new_item_state(driver).await?;
        close_clean_new_item_modal(driver).await?;
        assert_lane_add_preselects_state(driver).await?;
        assert_new_item_modal_dirty_leave_protection(driver).await?;
        open_new_item_modal(driver).await?;
        find(driver, By::Css("#new-item-modal select.agent-model-select")).await?;
        assert_source_contains(driver, "Project default").await?;
        set_input_value(driver, "#new-item-modal .crud-input-field", "Browser item").await?;
        replace_element_value(
            &find(
                driver,
                By::Css("#new-item-modal [data-rich-text-field='description'] .ProseMirror"),
            )
            .await?,
            "Created through browser-test\nSecond line",
            "new item description",
        )
        .await?;
        append_new_item_initial_label(driver, "area", "browser").await?;
        append_new_item_initial_label(driver, "needs-verification", "").await?;
        click_new_item_save(driver).await?;

        find_browser_item_card_link(driver).await?;
        assert_board_card_contains(driver, "Browser item", "area=browser").await?;
        assert_board_card_contains(driver, "Browser item", "needs-verification").await?;
        assert_source_contains(driver, "Created through browser-test").await?;
        assert_source_contains(driver, "state=idea").await?;

        Ok(())
    }
}

async fn open_new_item_modal(driver: &WebDriver) -> Result<(), Report> {
    let mut last_state = inspect_new_item_modal_state(driver).await?;
    for _ in 0..20 {
        if last_state.starts_with("modalVisible=true;") && last_state.contains("formReady=true") {
            return Ok(());
        }
        if !last_state.starts_with("modalVisible=true;") {
            click_css_after_modal_backdrops_clear(
                driver,
                ".lane:nth-child(1) .lane-add",
                "opening new item modal",
            )
            .await?;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        last_state = inspect_new_item_modal_state(driver).await?;
    }
    bail!("new item modal did not open: {last_state}");
}

async fn assert_lane_new_item_state(driver: &WebDriver) -> Result<(), Report> {
    let summary = select_value_and_options(
        &find(driver, By::Css("#new-item-modal select[name='state']")).await?,
        "lane-scoped new item state",
    )
    .await?;
    assert_that!(summary).is_equal_to("idea|idea".to_owned());
    Ok(())
}

async fn assert_new_item_modal_actions(driver: &WebDriver) -> Result<(), Report> {
    let modal = find(driver, By::Css("#new-item-modal")).await?;
    let header_button = modal
        .find(By::Css("leptonic-modal-header button"))
        .await
        .context("failed to find new-item header action")?;
    header_button
        .find(By::Css("svg"))
        .await
        .context("new-item header action did not contain an icon")?;
    assert_that!(
        header_button
            .text()
            .await
            .context("failed to read new-item header action")?
    )
    .is_empty();
    assert_that!(
        modal
            .find_all(By::Css("leptonic-modal-body .crud-nav button"))
            .await
            .context("failed to inspect new-item body actions")?
            .is_empty()
    )
    .is_true();
    let buttons = modal
        .find_all(By::Css("leptonic-modal-footer button"))
        .await
        .context("failed to inspect new-item footer actions")?;
    let mut labels = Vec::with_capacity(buttons.len());
    for button in buttons {
        labels.push(
            button
                .text()
                .await
                .context("failed to read new-item footer action")?,
        );
    }
    assert_that!(labels).is_equal_to(vec!["Cancel".to_owned(), "Speichern".to_owned()]);
    Ok(())
}

async fn assert_lane_add_preselects_state(driver: &WebDriver) -> Result<(), Report> {
    click_css_after_modal_backdrops_clear(
        driver,
        ".lane:nth-child(2) .lane-add",
        "opening lane-preselected new item modal",
    )
    .await?;
    find(driver, By::Css("#new-item-modal select[name='state']")).await?;
    let summary = select_value_and_options(
        &find(driver, By::Css("#new-item-modal select[name='state']")).await?,
        "lane-preselected new item state",
    )
    .await?;
    assert_that!(summary).is_equal_to("open|open".to_owned());
    close_clean_new_item_modal(driver).await?;
    Ok(())
}

async fn assert_new_item_modal_dirty_leave_protection(driver: &WebDriver) -> Result<(), Report> {
    open_new_item_modal(driver).await?;
    set_input_value(
        driver,
        "#new-item-modal .crud-input-field",
        "Unsaved modal title",
    )
    .await?;

    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-header button"),
    )
    .await?;
    find_leave_modal(driver)
        .await
        .context("new-item header close did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_new_item_title_value(driver, "Unsaved modal title").await?;

    click_backdrop(driver).await?;
    find_leave_modal(driver)
        .await
        .context("new-item backdrop click did not open the dirty-leave guard")?;
    click_leave_modal_accept(driver).await?;
    wait_for_new_item_modal_closed(driver).await?;

    open_new_item_modal(driver).await?;
    append_new_item_initial_label(driver, "area", "unsaved").await?;
    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-header button"),
    )
    .await?;
    find_leave_modal(driver)
        .await
        .context("label-dirty new-item header close did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_new_item_initial_label_value(driver, "area", "unsaved").await?;

    click_backdrop(driver).await?;
    find_leave_modal(driver)
        .await
        .context("label-dirty new-item backdrop click did not open the dirty-leave guard")?;
    click_leave_modal_accept(driver).await?;
    wait_for_new_item_modal_closed(driver).await?;
    Ok(())
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
