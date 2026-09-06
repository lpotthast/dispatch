use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt};
use rootcause::option_ext::OptionExt;

use super::common::*;

pub(crate) struct BoardGroupsAndApiTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for BoardGroupsAndApiTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("board groups and API labels render")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        seed_grouped_work_items(driver).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to reopen Dispatch board page")?;
        let grouped = find(driver, By::Css("[data-work-group-key='browser-review']")).await?;
        assert_that!(
            grouped
                .find_all(By::Css("article.card"))
                .await
                .context("failed to count grouped board cards")?
                .len()
        )
        .is_equal_to(2);
        assert_that!(
            grouped
                .text()
                .await
                .context("failed to read grouped work")?
        )
        .contains("Browser review");
        assert_live_updates_preserve_cards(driver).await?;
        assert_source_does_not_contain(driver, "Dispatch labels").await?;
        driver
            .goto(app.url("/api/docs?project=demo"))
            .await
            .context("failed to open Dispatch API page")?;
        find(driver, By::Css("section.dispatch-labels")).await?;
        assert_source_contains(driver, "dispatch:automation-blocked").await?;
        assert_source_contains(driver, "dispatch:feedback-requested").await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to reopen Dispatch board page after API check")?;

        Ok(())
    }
}

async fn assert_live_updates_preserve_cards(driver: &WebDriver) -> Result<(), Report> {
    let group = find(driver, By::Css("[data-work-group-key='browser-review']")).await?;
    let links = group
        .find_all(By::Css(".card-main-link"))
        .await
        .context("failed to read grouped links")?;
    let first = &links[0];
    let second = &links[1];
    let id = first
        .attr("data-board-item-id")
        .await
        .context("failed to read item id")?
        .context("missing item id")?;
    let lane = group
        .find(By::XPath("ancestor::section[contains(@class, 'lane')]"))
        .await
        .context("failed to find lane")?;
    let first_title = first
        .find(By::Css("h3"))
        .await
        .context("failed to find title")?;
    let response = browser_request(
        driver,
        reqwest::Method::PATCH,
        &format!("/api/projects/demo/items/{id}"),
    )
    .await?
    .json(&serde_json::json!({"title": "Updated grouped finding"}))
    .send()
    .await
    .context("failed to update grouped item")?;
    response_text(response, "grouped item update").await?;
    wait_until("live grouped title update", || async {
        let text = first_title
            .text()
            .await
            .context("title element was replaced during refresh")?;
        Ok((text == "Updated grouped finding").then_some(()))
    })
    .await?;
    assert_that!(
        find(driver, By::Css("[data-work-group-key='browser-review']"))
            .await?
            .element_id()
    )
    .is_equal_to(group.element_id().clone());
    assert_that!(
        second
            .is_displayed()
            .await
            .context("unchanged card was replaced")?
    )
    .is_true();
    assert_that!(lane.is_displayed().await.context("lane was replaced")?).is_true();
    assert_that!(
        find(
            driver,
            By::Css(&format!(".card-main-link[data-board-item-id='{id}']"))
        )
        .await?
        .element_id()
    )
    .is_equal_to(first.element_id().clone());

    // Add/remove a sibling without rebuilding the existing group and its cards.
    let added =
        create_browser_test_item(driver, "Live sibling", "Live sibling description").await?;
    let added_selector = format!(".card-main-link[data-board-item-id='{added}']");
    let added_link = find(driver, By::Css(&added_selector)).await?;
    let response = browser_request(
        driver,
        reqwest::Method::PATCH,
        &format!("/api/projects/demo/items/{added}"),
    )
    .await?
    .json(&serde_json::json!({"title": "Updated live sibling"}))
    .send()
    .await
    .context("failed to update sibling")?;
    response_text(response, "sibling update").await?;
    wait_until("live sibling title update", || async {
        let text = added_link
            .text()
            .await
            .context("sibling link was replaced during refresh")?;
        Ok(text.contains("Updated live sibling").then_some(()))
    })
    .await?;
    let response = browser_request(
        driver,
        reqwest::Method::PATCH,
        &format!("/api/projects/demo/items/{added}"),
    )
    .await?
    .json(&serde_json::json!({"state": "done"}))
    .send()
    .await
    .context("failed to move sibling")?;
    response_text(response, "sibling move").await?;
    wait_until("live sibling removal", || async {
        Ok(lane
            .find_all(By::Css(&added_selector))
            .await
            .context("failed to inspect moved sibling")?
            .is_empty()
            .then_some(()))
    })
    .await?;
    assert_that!(
        first
            .is_displayed()
            .await
            .context("grouped card was replaced after sibling removal")?
    )
    .is_true();
    assert_that!(
        second
            .is_displayed()
            .await
            .context("grouped sibling was replaced after removal")?
    )
    .is_true();
    assert_that!(
        group
            .is_displayed()
            .await
            .context("group was replaced after sibling removal")?
    )
    .is_true();
    Ok(())
}
