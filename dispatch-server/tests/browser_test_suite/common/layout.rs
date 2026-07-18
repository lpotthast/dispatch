use std::time::Duration;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, ElementRect, WebDriver, WebElement, components::SelectElement};
use leptos_browser_test::{Report, ResultExt};
use rootcause::option_ext::OptionExt;

use super::{element_value, find};

pub(crate) async fn layout_metrics(driver: &WebDriver) -> Result<serde_json::Value, Report> {
    Ok(driver
        .cdp()
        .send_raw("Page.getLayoutMetrics", ())
        .await
        .context("failed to inspect browser layout viewport")?)
}

pub(crate) async fn layout_viewport_size(driver: &WebDriver) -> Result<(f64, f64), Report> {
    let metrics = layout_metrics(driver).await?;
    let viewport = &metrics["cssLayoutViewport"];
    Ok((
        viewport["clientWidth"]
            .as_f64()
            .context("layout metrics did not contain viewport width")?,
        viewport["clientHeight"]
            .as_f64()
            .context("layout metrics did not contain viewport height")?,
    ))
}

pub(crate) async fn layout_viewport_offset(driver: &WebDriver) -> Result<(f64, f64), Report> {
    let metrics = layout_metrics(driver).await?;
    let viewport = &metrics["cssLayoutViewport"];
    Ok((
        viewport["pageX"]
            .as_f64()
            .context("layout metrics did not contain viewport x offset")?,
        viewport["pageY"]
            .as_f64()
            .context("layout metrics did not contain viewport y offset")?,
    ))
}

pub(crate) async fn css_pixels(
    element: &WebElement,
    property: &str,
    description: &str,
) -> Result<f64, Report> {
    let value = element
        .css_value(property)
        .await
        .context_with(|| format!("failed to inspect {description}"))?;
    Ok(value
        .strip_suffix("px")
        .unwrap_or(&value)
        .parse::<f64>()
        .context_with(|| format!("failed to parse {description}: {value:?}"))?)
}

pub(crate) async fn element_numeric_property(
    element: &WebElement,
    property: &str,
    description: &str,
) -> Result<f64, Report> {
    let value = element
        .prop(property)
        .await
        .context_with(|| format!("failed to inspect {description}"))?
        .context_with(|| format!("{description} did not expose {property}"))?;
    Ok(value
        .parse::<f64>()
        .context_with(|| format!("failed to parse {description} {property}: {value:?}"))?)
}

pub(crate) async fn select_value_and_options(
    element: &WebElement,
    description: &str,
) -> Result<String, Report> {
    let options = SelectElement::new(element)
        .await
        .context_with(|| format!("failed to inspect {description}"))?
        .options()
        .await
        .context_with(|| format!("failed to read {description} options"))?;
    let mut values = Vec::with_capacity(options.len());
    for option in options {
        values.push(
            option
                .attr("value")
                .await
                .context_with(|| format!("failed to read {description} option"))?
                .unwrap_or_default(),
        );
    }
    Ok(format!(
        "{}|{}",
        element_value(element, description).await?,
        values.join(",")
    ))
}

pub(crate) fn rects_overlap(left: &ElementRect, right: &ElementRect) -> bool {
    left.x < right.x + right.width
        && left.x + left.width > right.x
        && left.y < right.y + right.height
        && left.y + left.height > right.y
}

pub(crate) async fn modal_layer_is_clear(
    driver: &WebDriver,
    modal_selector: Option<&str>,
) -> Result<bool, Report> {
    if let Some(selector) = modal_selector {
        for modal in driver
            .find_all(By::Css(selector))
            .await
            .context("failed to inspect modal visibility")?
        {
            if modal
                .is_displayed()
                .await
                .context("failed to inspect modal visibility")?
            {
                return Ok(false);
            }
        }
    }
    for backdrop in driver
        .find_all(By::Css("leptonic-modal-backdrop"))
        .await
        .context("failed to inspect modal backdrops")?
    {
        if backdrop
            .is_displayed()
            .await
            .context("failed to inspect modal backdrop visibility")?
            && backdrop
                .css_value("pointer-events")
                .await
                .context("failed to inspect modal backdrop pointer events")?
                != "none"
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) async fn drag_drawer_to_percent(
    driver: &WebDriver,
    handle: &WebElement,
    percent: f64,
) -> Result<i64, Report> {
    let layout = find(driver, By::Css(".board-inspector-layout")).await?;
    let layout_rect = layout
        .rect()
        .await
        .context("failed to inspect Board drawer layout width")?;
    let handle_rect = handle
        .rect()
        .await
        .context("failed to inspect Board drawer resize handle")?;
    let start_x = handle_rect.x + handle_rect.width / 2.0;
    let target_x = layout_rect.x + layout_rect.width * (1.0 - percent / 100.0);
    driver
        .action_chain()
        .drag_and_drop_element_by_offset(handle, (target_x - start_x).round() as i64, 0)
        .release()
        .perform()
        .await
        .context("failed to drag the Board drawer resize handle")?;
    tokio::time::sleep(Duration::from_millis(240)).await;
    let drawer_width = find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown"),
    )
    .await?
    .rect()
    .await
    .context("failed to inspect resized Board drawer")?
    .width;
    Ok((drawer_width / layout_rect.width * 100.0).round() as i64)
}
pub(crate) async fn assert_main_content_scrolls_clear_of_workspace_dock(
    driver: &WebDriver,
) -> Result<(), Report> {
    let workspace_height = find(driver, By::Css(".workspace-dock"))
        .await?
        .rect()
        .await
        .context("failed to inspect workspace dock height")?
        .height;
    let body_padding = css_pixels(
        &find(driver, By::Css("body")).await?,
        "padding-bottom",
        "body workspace-dock clearance",
    )
    .await?;
    let scroll_padding = css_pixels(
        &find(driver, By::Css("html")).await?,
        "scroll-padding-bottom",
        "document workspace-dock clearance",
    )
    .await?;
    assert_that!(body_padding + 1.0 >= workspace_height).is_true();
    assert_that!(scroll_padding + 1.0 >= workspace_height).is_true();
    Ok(())
}
