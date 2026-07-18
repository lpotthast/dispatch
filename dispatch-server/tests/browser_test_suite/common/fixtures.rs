use assertr::prelude::*;
use browser_test::thirtyfour::WebDriver;
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

use super::{DispatchTestApp, browser_request, response_text};

pub(crate) async fn open_projects_without_test_projects(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    driver
        .goto(app.url("/projects"))
        .await
        .context("failed to open Dispatch projects page")?;
    delete_project_if_present(driver, "demo-alt").await?;
    delete_project_if_present(driver, "demo").await?;
    Ok(())
}

pub(crate) async fn reset_test_projects(
    driver: &WebDriver,
    app: &DispatchTestApp,
    include_alternate: bool,
) -> Result<(), Report> {
    open_projects_without_test_projects(driver, app).await?;
    create_project(driver).await?;
    if include_alternate {
        create_alternate_project(driver).await?;
    }
    Ok(())
}

async fn delete_project_if_present(driver: &WebDriver, project: &str) -> Result<(), Report> {
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        &format!("/projects/{project}/delete"),
    )
    .await?
    .send()
    .await
    .context_with(|| format!("failed to reset browser-test project {project:?}"))?;
    if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    response_text(response, &format!("browser-test project {project:?} reset")).await?;
    Ok(())
}

pub(crate) async fn create_project(driver: &WebDriver) -> Result<(), Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/projects")
        .await?
        .form(&[("name", "demo"), ("display_name", "Demo"), ("path", ".")])
        .send()
        .await
        .context("failed to create project through browser-test setup request")?;
    response_text(response, "project setup").await?;
    Ok(())
}

pub(crate) async fn create_alternate_project(driver: &WebDriver) -> Result<(), Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/projects")
        .await?
        .form(&[
            ("name", "demo-alt"),
            ("display_name", "Demo Alt"),
            ("path", "."),
        ])
        .send()
        .await
        .context("failed to create alternate project through browser-test setup request")?;
    response_text(response, "alternate-project setup").await?;
    Ok(())
}

pub(crate) async fn create_browser_test_item(
    driver: &WebDriver,
    title: &str,
    description: &str,
) -> Result<i64, Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/api/projects/demo/items")
        .await?
        .json(&serde_json::json!({
            "title": title,
            "description": description,
            "state": "open",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null
        }))
        .send()
        .await
        .context("failed to create item through browser-test API request")?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to read browser-test item setup response")?;
    if !status.is_success() {
        bail!("browser-test item creation failed with {status}: {body}");
    }
    Ok(body["id"]
        .as_i64()
        .context_with(|| format!("browser-test item response did not contain an id: {body}"))?)
}

pub(crate) async fn test_project_id(driver: &WebDriver) -> Result<i64, Report> {
    let response = browser_request(driver, reqwest::Method::GET, "/api/projects/demo")
        .await?
        .send()
        .await
        .context("failed to read browser-test project")?;
    let status = response.status();
    let project: serde_json::Value = response
        .json()
        .await
        .context("failed to decode browser-test project")?;
    if !status.is_success() {
        bail!("browser-test project request failed with {status}: {project}");
    }
    Ok(project["id"]
        .as_i64()
        .context_with(|| format!("browser-test project response had no id: {project}"))?)
}

pub(crate) async fn create_labeled_browser_test_item(driver: &WebDriver) -> Result<i64, Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/api/projects/demo/items")
        .await?
        .json(&serde_json::json!({
            "title": "Browser item",
            "description": "Created through browser-test\nSecond line",
            "state": "idea",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null,
            "initial_labels": [
                { "key": "area", "value": "browser" },
                { "key": "needs-verification", "value": null }
            ]
        }))
        .send()
        .await
        .context("failed to create labeled browser-test item")?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to read labeled browser-test item response")?;
    if !status.is_success() {
        bail!("labeled browser-test item creation failed with {status}: {body}");
    }
    Ok(body["id"]
        .as_i64()
        .context_with(|| format!("labeled browser-test item response had no id: {body}"))?)
}
pub(crate) async fn seed_grouped_work_items(driver: &WebDriver) -> Result<(), Report> {
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        "/api/projects/demo/work-groups",
    )
    .await?
    .json(&serde_json::json!({
        "key": "browser-review",
        "name": "Browser review"
    }))
    .send()
    .await
    .context("failed to create grouped-work fixture")?;
    response_text(response, "work-group creation").await?;
    let first = create_browser_test_item(
        driver,
        "Grouped finding one",
        "First grouped browser-test item",
    )
    .await?;
    let second = create_browser_test_item(
        driver,
        "Grouped finding two",
        "Second grouped browser-test item",
    )
    .await?;
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        "/api/projects/demo/work-groups/browser-review/items",
    )
    .await?
    .json(&serde_json::json!({ "item_ids": [first, second] }))
    .send()
    .await
    .context("failed to attach grouped-work fixtures")?;
    let status = response.status();
    let group: serde_json::Value = response
        .json()
        .await
        .context("failed to read grouped work-item seed result")?;
    if !status.is_success() {
        bail!("grouped-work fixture failed with {status}: {group}");
    }
    assert_that!(group["item_count"].as_i64()).is_equal_to(Some(2));
    Ok(())
}
