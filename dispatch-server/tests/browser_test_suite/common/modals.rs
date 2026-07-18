use std::time::Duration;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use leptos_browser_test::{Report, ResultExt, bail};

use super::{
    click, editable_element, element_value, find, modal_layer_is_clear, replace_element_value,
    wait_until,
};

pub(crate) async fn close_clean_new_item_modal(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::Css("#new-item-modal leptonic-modal-footer button"),
    )
    .await?;
    wait_for_new_item_modal_closed(driver).await
}

pub(crate) async fn click_new_item_save(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath("//leptonic-modal[@id='new-item-modal']//button[contains(., 'Speichern')]"),
    )
    .await
}

pub(crate) async fn find_leave_modal(driver: &WebDriver) -> Result<(), Report> {
    find(
        driver,
        By::XPath("//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]"),
    )
    .await
    .map(|_| ())
}

pub(crate) async fn click_leave_modal_cancel(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath(
            "//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]//button[contains(., 'Zurück')]",
        ),
    )
    .await?;
    wait_for_leave_modal_closed(driver).await
}

pub(crate) async fn click_leave_modal_accept(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::XPath(
            "//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]//button[contains(., 'Verlassen')]",
        ),
    )
    .await?;
    wait_for_leave_modal_closed(driver).await
}

pub(crate) async fn wait_for_leave_modal_closed(driver: &WebDriver) -> Result<(), Report> {
    wait_until("dirty-leave modal close", || async {
        let modals = driver
            .find_all(By::XPath(
                "//leptonic-modal[contains(., 'Ungespeicherte Änderungen')]",
            ))
            .await
            .context("failed to inspect dirty-leave modal close state")?;
        for modal in modals {
            if modal
                .is_displayed()
                .await
                .context("failed to inspect dirty-leave modal visibility")?
            {
                return Ok(None);
            }
        }
        Ok(Some(()))
    })
    .await?;
    Ok(())
}

pub(crate) async fn assert_work_item_create_title_value(
    driver: &WebDriver,
    expected: &str,
) -> Result<(), Report> {
    let value = element_value(
        &editable_element(
            &find(
                driver,
                By::Css("[data-crudkit-leptos='work-items'] .crud-input-field"),
            )
            .await?,
        )
        .await?,
        "work-item create draft",
    )
    .await?;
    assert_that!(value).is_equal_to(expected.to_owned());
    Ok(())
}

pub(crate) async fn assert_item_detail_title_value(
    driver: &WebDriver,
    expected: &str,
) -> Result<(), Report> {
    let value = element_value(
        &editable_element(&find(driver, By::Css("section.item-settings .crud-input-field")).await?)
            .await?,
        "item detail draft",
    )
    .await?;
    assert_that!(value).is_equal_to(expected.to_owned());
    Ok(())
}

pub(crate) async fn assert_new_item_title_value(
    driver: &WebDriver,
    expected: &str,
) -> Result<(), Report> {
    let value = element_value(
        &editable_element(&find(driver, By::Css("#new-item-modal .crud-input-field")).await?)
            .await?,
        "new item title draft",
    )
    .await?;
    assert_that!(value).is_equal_to(expected.to_owned());
    Ok(())
}

pub(crate) async fn append_new_item_initial_label(
    driver: &WebDriver,
    key: &str,
    value: &str,
) -> Result<(), Report> {
    let before = driver
        .find_all(By::Css("#new-item-modal .initial-label-row"))
        .await
        .context("failed to count new-item initial labels")?
        .len();
    click(driver, By::Css("#new-item-modal .initial-label-add")).await?;
    let row = wait_until("new item initial-label row", || async {
        let rows = driver
            .find_all(By::Css("#new-item-modal .initial-label-row"))
            .await
            .context("failed to inspect new-item initial labels")?;
        Ok((rows.len() > before)
            .then(|| rows.last().cloned())
            .flatten())
    })
    .await?;
    replace_element_value(
        &row.find(By::Css(".initial-label-key"))
            .await
            .context("failed to find initial-label key")?,
        key,
        "initial-label key",
    )
    .await?;
    replace_element_value(
        &row.find(By::Css(".initial-label-value"))
            .await
            .context("failed to find initial-label value")?,
        value,
        "initial-label value",
    )
    .await?;
    Ok(())
}

pub(crate) async fn assert_new_item_initial_label_value(
    driver: &WebDriver,
    expected_key: &str,
    expected_value: &str,
) -> Result<(), Report> {
    let row = find(driver, By::Css("#new-item-modal .initial-label-row")).await?;
    let summary = format!(
        "{}|{}",
        element_value(
            &row.find(By::Css(".initial-label-key"))
                .await
                .context("failed to find initial-label key draft")?,
            "initial-label key draft",
        )
        .await?,
        element_value(
            &row.find(By::Css(".initial-label-value"))
                .await
                .context("failed to find initial-label value draft")?,
            "initial-label value draft",
        )
        .await?,
    );
    assert_that!(summary).is_equal_to(format!("{expected_key}|{expected_value}"));
    Ok(())
}

pub(crate) async fn assert_board_card_contains(
    driver: &WebDriver,
    title: &str,
    expected: &str,
) -> Result<(), Report> {
    let card = find(
        driver,
        By::XPath(format!(
            "//article[contains(@class, 'card')][.//a[contains(., {title:?})]]"
        )),
    )
    .await?;
    assert_that!(
        card.text()
            .await
            .context("failed to inspect board card labels")?
    )
    .contains(expected);
    Ok(())
}

pub(crate) async fn click_backdrop(driver: &WebDriver) -> Result<(), Report> {
    driver
        .action_chain()
        .move_to(4, 4)
        .click()
        .perform()
        .await
        .context("failed to click modal backdrop")?;
    Ok(())
}

pub(crate) async fn wait_for_new_item_modal_closed(driver: &WebDriver) -> Result<(), Report> {
    wait_until("new item modal close", || async {
        Ok(modal_layer_is_clear(driver, Some("#new-item-modal"))
            .await?
            .then_some(()))
    })
    .await
}

pub(crate) async fn wait_for_no_modal_backdrop_blocking(
    driver: &WebDriver,
    context: &str,
) -> Result<(), Report> {
    wait_until(context, || async {
        Ok(modal_layer_is_clear(driver, None).await?.then_some(()))
    })
    .await?;
    Ok(())
}

pub(crate) async fn click_css_after_modal_backdrops_clear(
    driver: &WebDriver,
    selector: &str,
    context: &str,
) -> Result<(), Report> {
    let mut last_error = None;
    for _ in 0..20 {
        wait_for_no_modal_backdrop_blocking(driver, context).await?;
        let element = find(driver, By::Css(selector)).await?;
        element
            .scroll_into_view()
            .await
            .context("failed to scroll browser-test element into view")?;
        driver
            .action_chain()
            .move_to_element_center(&element)
            .perform()
            .await
            .context("failed to move pointer to browser-test element")?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        match element.click().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error.to_string());
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }

    bail!(
        "failed to click browser-test element {selector:?} while {context}: {}",
        last_error.unwrap_or_else(|| "no click attempt was made".to_owned())
    );
}

pub(crate) async fn inspect_new_item_modal_state(driver: &WebDriver) -> Result<String, Report> {
    let modals = driver
        .find_all(By::Css("leptonic-modal#new-item-modal"))
        .await
        .context("failed to inspect new item modal state")?;
    let modal = modals.first();
    let modal_visible = if let Some(modal) = modal {
        modal
            .is_displayed()
            .await
            .context("failed to inspect new item modal visibility")?
    } else {
        false
    };
    let form_ready = if let Some(modal) = modal {
        let fields = modal
            .find_all(By::Css(".crud-input-field"))
            .await
            .context("failed to inspect new item title field")?;
        let title_ready = if let Some(field) = fields.first() {
            editable_element(field).await.is_ok()
        } else {
            false
        };
        title_ready
            && !modal
                .find_all(By::Css("select[name='state']"))
                .await
                .context("failed to inspect new item state field")?
                .is_empty()
    } else {
        false
    };
    Ok(format!(
        "modalVisible={modal_visible}; modal={}; formReady={form_ready}",
        modal.is_some()
    ))
}
