use std::{future::Future, time::Duration};

use browser_test::thirtyfour::{By, Key, WebDriver, WebElement, components::SelectElement};
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

pub(crate) async fn wait_until<T, F, Fut>(description: &str, mut inspect: F) -> Result<T, Report>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>, Report>>,
{
    for _ in 0..50 {
        if let Some(value) = inspect().await? {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("timed out waiting for {description}")
}

pub(crate) async fn element_value(
    element: &WebElement,
    description: &str,
) -> Result<String, Report> {
    Ok(element
        .prop("value")
        .await
        .context_with(|| format!("failed to read {description} value"))?
        .unwrap_or_default())
}

pub(crate) async fn select_value(
    element: &WebElement,
    value: &str,
    description: &str,
) -> Result<(), Report> {
    SelectElement::new(element)
        .await
        .context_with(|| format!("failed to inspect {description} select"))?
        .select_by_value(value)
        .await
        .context_with(|| format!("failed to select {value:?} in {description}"))?;
    Ok(())
}

pub(crate) async fn assert_source_contains(
    driver: &WebDriver,
    expected: &str,
) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    if !source.contains(expected) {
        bail!("page source did not contain {expected:?}");
    }
    Ok(())
}

pub(crate) async fn assert_source_does_not_contain(
    driver: &WebDriver,
    unexpected: &str,
) -> Result<(), Report> {
    let source = driver
        .source()
        .await
        .context("failed to read page source")?;
    if source.contains(unexpected) {
        bail!("page source unexpectedly contained {unexpected:?}");
    }
    Ok(())
}

pub(crate) async fn find(driver: &WebDriver, by: By) -> Result<WebElement, Report> {
    let mut last_error = None;
    for _ in 0..30 {
        match driver.find(by.clone()).await {
            Ok(element) => return Ok(element),
            Err(error) => last_error = Some(error.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let current_url = driver
        .current_url()
        .await
        .map(|url| url.to_string())
        .unwrap_or_else(|url_err| format!("failed to read current URL: {url_err}"));
    let source = driver
        .source()
        .await
        .unwrap_or_else(|source_err| format!("failed to read page source: {source_err}"));
    let source_prefix = source.chars().take(4_000).collect::<String>();
    bail!(
        "failed to find browser-test element at {current_url}: {}; source prefix: {source_prefix}",
        last_error.unwrap_or_else(|| "no find attempt completed".to_owned())
    );
}

pub(crate) async fn click(driver: &WebDriver, by: By) -> Result<(), Report> {
    let target = format!("{by:?}");
    let mut last_error = None;
    for _ in 0..3 {
        let element = find(driver, by.clone()).await?;
        match element.click().await {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let element = find(driver, by.clone()).await?;
        if element.scroll_into_view().await.is_ok() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            match element.click().await {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    bail!(
        "failed to click browser-test element {target}: {}",
        last_error.unwrap_or_else(|| "no click attempt completed".to_owned())
    )
}

pub(crate) async fn click_element(element: &WebElement, description: &str) -> Result<(), Report> {
    if element.click().await.is_ok() {
        return Ok(());
    }
    element
        .scroll_into_view()
        .await
        .context_with(|| format!("failed to scroll {description} into view"))?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    if element.click().await.is_ok() {
        return Ok(());
    }
    element
        .send_keys(Key::Enter)
        .await
        .context_with(|| format!("failed to activate {description}"))?;
    Ok(())
}

pub(crate) async fn set_input_value(
    driver: &WebDriver,
    selector: &str,
    value: &str,
) -> Result<(), Report> {
    let fields = driver
        .find_all(By::Css(selector))
        .await
        .context("failed to find browser-test input wrapper")?;
    for field in fields {
        if let Ok(input) = editable_element(&field).await
            && input.is_enabled().await.unwrap_or(false)
            && input.attr("readonly").await.unwrap_or(None).is_none()
            && input.attr("type").await.unwrap_or(None).as_deref() != Some("hidden")
        {
            return replace_element_value(&input, value, selector).await;
        }
    }
    bail!("failed to find editable browser-test input for {selector:?}")
}

pub(crate) async fn replace_element_value(
    element: &WebElement,
    value: &str,
    description: &str,
) -> Result<(), Report> {
    if element
        .tag_name()
        .await
        .context_with(|| format!("failed to inspect {description} element type"))?
        == "select"
    {
        return select_value(element, value, description).await;
    }
    element
        .clear()
        .await
        .context_with(|| format!("failed to clear {description}"))?;
    element
        .send_keys(value)
        .await
        .context_with(|| format!("failed to enter {description}"))?;
    Ok(())
}

pub(crate) async fn editable_element(field: &WebElement) -> Result<WebElement, Report> {
    if matches!(
        field
            .tag_name()
            .await
            .context("failed to inspect editable field type")?
            .as_str(),
        "input" | "textarea" | "select"
    ) {
        return Ok(field.clone());
    }
    let controls = field
        .find_all(By::Css("input, textarea, select"))
        .await
        .context("failed to inspect wrapped editable field")?;
    Ok(controls
        .into_iter()
        .find(|control| control.element_id() != field.element_id())
        .context("editable field wrapper did not contain an input")?)
}

pub(crate) async fn current_item_id(driver: &WebDriver) -> Result<i64, Report> {
    let url = driver
        .current_url()
        .await
        .context("failed to read current item URL")?;
    Ok(url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .context_with(|| format!("current URL did not identify an item: {url}"))?
        .parse::<i64>()
        .context_with(|| format!("failed to parse item id from {url}"))?)
}

pub(crate) async fn clear_board_service_cache(driver: &WebDriver) -> Result<(), Report> {
    let current_url = driver
        .current_url()
        .await
        .context("failed to read browser URL for Board cache invalidation")?;
    let storage_id = serde_json::json!({
        "securityOrigin": current_url.origin().ascii_serialization(),
        "isLocalStorage": true
    });
    for key in ["dispatch.query.board.v2", "dispatch.query.board-items.v2"] {
        driver
            .cdp()
            .send_raw(
                "DOMStorage.removeDOMStorageItem",
                serde_json::json!({
                    "storageId": storage_id,
                    "key": key
                }),
            )
            .await
            .context_with(|| format!("failed to invalidate Board cache {key:?}"))?;
    }
    Ok(())
}

pub(crate) async fn send_keys(driver: &WebDriver, by: By, value: &str) -> Result<(), Report> {
    find(driver, by)
        .await?
        .send_keys(value)
        .await
        .context("failed to type into browser-test element")?;
    Ok(())
}
