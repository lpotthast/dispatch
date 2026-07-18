use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};

use super::common::*;

pub(crate) struct ProjectLifecycleTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for ProjectLifecycleTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("same-key project recreation clears selection and data")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        create_browser_test_item(
            driver,
            "Deleted project item",
            "Must not survive recreation",
        )
        .await?;
        driver
            .goto(app.url("/projects?project=demo"))
            .await
            .context("failed to select project before lifecycle check")?;
        assert_project_delete_recreate_clears_selection_and_data(driver).await
    }
}

async fn assert_project_delete_recreate_clears_selection_and_data(
    driver: &WebDriver,
) -> Result<(), Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/projects/demo/delete")
        .await?
        .send()
        .await
        .context("failed to delete project through browser-test request")?;
    response_text(response, "project deletion").await?;
    wait_until("deleted active project to clear selection", || async {
        let url = driver
            .current_url()
            .await
            .context("failed to read URL after project deletion")?;
        let no_projects = !driver
            .find_all(By::Css(".project-switcher-empty .project-empty"))
            .await
            .context("failed to inspect empty project switcher")?
            .is_empty();
        let unselected = if let Some(select) = driver
            .find_all(By::Css("#project-switcher-choice"))
            .await
            .context("failed to inspect unselected project switcher")?
            .first()
        {
            element_value(select, "project switcher").await?.is_empty()
        } else {
            false
        };
        Ok(
            (url.path() == "/projects" && url.query().is_none() && (no_projects || unselected))
                .then_some(()),
        )
    })
    .await?;

    let response = browser_request(driver, reqwest::Method::POST, "/projects")
        .await?
        .form(&[
            ("name", "demo"),
            ("display_name", "Recreated Demo"),
            ("path", ".."),
        ])
        .send()
        .await
        .context("failed to recreate project through browser-test request")?;
    response_text(response, "project recreation").await?;
    wait_until("same-key project to remain unselected", || async {
        let url = driver
            .current_url()
            .await
            .context("failed to read URL after project recreation")?;
        let select = find(driver, By::Css("#project-switcher-choice")).await?;
        let options = select
            .find_all(By::Css("option[value='demo']"))
            .await
            .context("failed to inspect recreated project option")?;
        Ok((!options.is_empty()
            && element_value(&select, "project switcher").await?.is_empty()
            && url.query().is_none())
        .then_some(()))
    })
    .await?;

    let response = browser_request(driver, reqwest::Method::GET, "/api/projects/demo/items")
        .await?
        .send()
        .await
        .context("failed to list recreated project items")?;
    let status = response.status();
    let items: serde_json::Value = response
        .json()
        .await
        .context("failed to read recreated project items")?;
    if !status.is_success() {
        bail!("recreated project item list failed with {status}: {items}");
    }
    assert_that!(items.as_array().map(Vec::len)).is_equal_to(Some(0));
    Ok(())
}
