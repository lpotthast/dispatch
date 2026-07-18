use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt};

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
