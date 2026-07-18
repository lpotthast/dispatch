use std::{borrow::Cow, env, path::Path};

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

use super::common::*;

pub(crate) struct AutomationAdministrationTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for AutomationAdministrationTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("automation administration configures bundles and triggers")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        driver
            .goto(app.url("/automation?project=demo"))
            .await
            .context("failed to open Dispatch automation page")?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Automation");
        assert_source_contains(driver, "Automation policy").await?;
        find(driver, By::Css("#project-max-read-only-agents")).await?;
        assert_source_contains(driver, "Read-only agents").await?;
        assert_source_contains(driver, "Auto-Commit").await?;
        find(driver, By::Css("#project-auto-commit")).await?;
        find(driver, By::Css("#project-commit-standard")).await?;
        find(
            driver,
            By::Css("#project-revert-strategy option[value='git_reset']"),
        )
        .await?;
        assert_source_does_not_contain(driver, "Maintenance").await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='automation-triggers'] .crud-nav"),
        )
        .await?;
        find(driver, By::Css(".trigger-runs")).await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"automation-triggers\"").await?;
        assert_source_contains(driver, "Work-consuming automations").await?;
        assert_source_contains(driver, "Work-producing automations").await?;
        assert_automation_section_layout(driver).await?;
        assert_source_contains(driver, "Mutability").await?;
        assert_automation_bundle_controls(driver).await?;
        assert_personality_revision_restore(driver).await?;
        assert_structured_automation_configuration_editors(driver).await?;
        assert_source_contains(driver, "No automation selected").await?;
        assert_source_does_not_contain(driver, "Create trigger").await?;
        assert_source_does_not_contain(driver, "trigger-edit-form").await?;

        create_trigger(driver).await?;
        driver
            .goto(app.url("/automation?project=demo"))
            .await
            .context("failed to reload Dispatch automation page after automation creation")?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='automation-triggers'] .crud-nav"),
        )
        .await?;
        find(driver, By::XPath("//*[contains(text(), 'refine-new')]")).await?;
        assert_source_contains(driver, "refine-new").await?;

        Ok(())
    }
}

async fn assert_automation_bundle_controls(driver: &WebDriver) -> Result<(), Report> {
    find(driver, By::Css("[data-testid='automation-bundles']")).await?;
    find(driver, By::Css(".automation-bundle-yaml")).await?;
    let bundle_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/automation/engineering-review.yaml")
        .canonicalize()
        .context("failed to canonicalize the example automation bundle path")?;
    find(driver, By::Css("[data-testid='automation-bundle-file']"))
        .await?
        .send_keys(bundle_path.to_string_lossy().as_ref())
        .await
        .context("failed to choose an automation bundle YAML file")?;
    let loaded = wait_for_bundle_editor(driver, "bundle_key: engineering-review").await?;
    assert_that!(loaded).contains("created_items_share_group: true");
    find(
        driver,
        By::Css(".automation-bundle-actions input[type='checkbox']"),
    )
    .await?;
    set_input_value(
        driver,
        ".automation-bundle-yaml",
        "schema_version: 1\nbundle_key: browser-test\ndisplay_name: Browser test\n",
    )
    .await?;
    click(
        driver,
        By::XPath("//*[@data-testid='automation-bundles']//button[normalize-space()='Validate']"),
    )
    .await
    .context("failed to validate bundle through automation UI")?;
    let validation = wait_for_bundle_result(driver, "manifest_hash").await?;
    assert_that!(validation).contains("browser-test");

    click(
        driver,
        By::XPath("//*[@data-testid='automation-bundles']//button[normalize-space()='Diff']"),
    )
    .await
    .context("failed to diff bundle through automation UI")?;
    let diff = wait_for_bundle_result(driver, "objects").await?;
    assert_that!(diff).contains("browser-test");

    click(
        driver,
        By::XPath("//*[@data-testid='automation-bundles']//button[normalize-space()='Apply']"),
    )
    .await
    .context("failed to apply bundle through automation UI")?;
    let apply = wait_for_bundle_result(driver, "apply_id").await?;
    assert_that!(apply).contains("\"status\": \"applied\"");
    wait_for_bundle_inventory(driver, "browser-test", true).await?;
    click(
        driver,
        By::Css("[data-bundle-key='browser-test'] button.danger"),
    )
    .await
    .context("failed to arm bundle removal")?;
    let remove = find(
        driver,
        By::Css("[data-bundle-key='browser-test'] button.danger"),
    )
    .await?;
    assert_that!(
        remove
            .text()
            .await
            .context("failed to read removal button")?
    )
    .is_equal_to("Confirm removal".to_owned());
    click(
        driver,
        By::Css("[data-bundle-key='browser-test'] button.danger"),
    )
    .await
    .context("failed to confirm bundle removal")?;
    let removal = wait_for_bundle_result(driver, "\"status\": \"removed\"").await?;
    assert_that!(removal).contains("\"status\": \"removed\"");
    wait_for_bundle_inventory(driver, "browser-test", false).await?;
    Ok(())
}

async fn wait_for_bundle_editor(driver: &WebDriver, expected: &str) -> Result<String, Report> {
    wait_until("selected bundle YAML", || async {
        let value = element_value(
            &find(driver, By::Css(".automation-bundle-yaml")).await?,
            "bundle editor",
        )
        .await?;
        Ok(value.contains(expected).then_some(value))
    })
    .await
}

async fn wait_for_bundle_inventory(
    driver: &WebDriver,
    bundle_key: &str,
    present: bool,
) -> Result<(), Report> {
    wait_until("installed bundle inventory", || async {
        let found = !driver
            .find_all(By::Css(format!("[data-bundle-key='{bundle_key}']")))
            .await
            .context("failed to inspect installed bundle inventory")?
            .is_empty();
        Ok((found == present).then_some(()))
    })
    .await?;
    Ok(())
}

async fn wait_for_bundle_result(driver: &WebDriver, expected: &str) -> Result<String, Report> {
    wait_until("automation bundle result", || async {
        let text = find(driver, By::Css(".automation-bundle-result"))
            .await?
            .text()
            .await
            .context("failed to read automation bundle result")?;
        Ok((text.contains(expected) || text.contains("failed:")).then_some(text))
    })
    .await
}

async fn assert_automation_section_layout(driver: &WebDriver) -> Result<(), Report> {
    let container = find(driver, By::Css(".automation-sections")).await?;
    assert_that!(
        container
            .css_value("display")
            .await
            .context("failed to inspect automation section display")?
    )
    .is_equal_to("grid");
    let sections = container
        .find_all(By::Css(":scope > section"))
        .await
        .context("failed to inspect automation sections")?;
    let mut headings = Vec::with_capacity(sections.len());
    let mut previous_bottom = None;
    for section in &sections {
        headings.push(
            section
                .find(By::Css("h2"))
                .await
                .context("failed to find automation section heading")?
                .text()
                .await
                .context("failed to read automation section heading")?,
        );
        let rect = section
            .rect()
            .await
            .context("failed to inspect automation section geometry")?;
        if let Some(bottom) = previous_bottom {
            assert_that!(rect.y > bottom).is_true();
        }
        previous_bottom = Some(rect.y + rect.height);
    }
    let producing_index = headings
        .iter()
        .position(|heading| heading == "Work-producing automations")
        .context("missing work-producing automation section")?;
    let policy_index = headings
        .iter()
        .position(|heading| heading == "Automation policy")
        .context("missing automation policy section")?;
    assert_that!(policy_index).is_equal_to(producing_index + 1);
    Ok(())
}

async fn assert_structured_automation_configuration_editors(
    driver: &WebDriver,
) -> Result<(), Report> {
    click(
        driver,
        By::Css("section.automation-triggers .crudkit-automation-triggers .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css(".postconditions-editor[data-postconditions-editor='structured']"),
    )
    .await?;
    let consuming = find(
        driver,
        By::XPath(
            "//section[contains(@class, 'automation-triggers')][.//h2[normalize-space()='Work-consuming automations']][.//*[contains(@class, 'crudkit-automation-triggers')]]",
        ),
    )
    .await?;
    consuming
        .find(By::Css(
            "[data-condition-editor='structured'], [data-lane-filter-editor='structured']",
        ))
        .await
        .context("failed to find consuming selector editor")?;
    let add_outcome = consuming
        .find(By::Css("[data-postconditions-add-outcome='true']"))
        .await
        .context("failed to find add-outcome control")?;
    click_element(&add_outcome, "add postcondition outcome").await?;
    let disposition = consuming
        .find(By::Css("[data-postcondition-disposition='true']"))
        .await
        .context("failed to find postcondition disposition")?;
    select_value(&disposition, "finished", "postcondition disposition").await?;
    let add_label = consuming
        .find(By::Css("[data-postcondition-add-label='true']"))
        .await
        .context("failed to find postcondition add-label control")?;
    click_element(&add_label, "add postcondition label").await?;
    let add_created_items = consuming
        .find(By::Css("[data-postcondition-add-created-items='true']"))
        .await
        .context("failed to find created-items postcondition control")?;
    click_element(&add_created_items, "add created-items postcondition").await?;
    for selector in [
        "[data-postcondition-outcome='0']",
        "[data-postcondition-label='0']",
        "[data-postcondition-created-items='true']",
        "[data-postcondition-created-items='true'] [data-condition-editor='structured']",
    ] {
        consuming.find(By::Css(selector)).await.context_with(|| {
            format!("failed to find structured postcondition control {selector}")
        })?;
    }

    click(
        driver,
        By::Css(
            "section.automation-triggers + section.automation-triggers .crudkit-automation-triggers .crud-nav button",
        ),
    )
    .await?;
    find(
        driver,
        By::Css(".produced-work-editor[data-produced-work-editor='structured']"),
    )
    .await?;
    let producing = find(
        driver,
        By::XPath(
            "//section[contains(@class, 'automation-triggers')][.//h2[normalize-space()='Work-producing automations']][.//*[contains(@class, 'crudkit-automation-triggers')]]",
        ),
    )
    .await?;
    replace_element_value(
        &producing
            .find(By::Css("[data-produced-title='true']"))
            .await
            .context("failed to find produced-work title")?,
        "Browser-produced item",
        "produced-work title",
    )
    .await?;
    replace_element_value(
        &producing
            .find(By::Css("[data-produced-state='true']"))
            .await
            .context("failed to find produced-work state")?,
        "queued",
        "produced-work state",
    )
    .await?;
    let deduplication = producing
        .find(By::Css("[data-produced-deduplication='true']"))
        .await
        .context("failed to find produced-work deduplication")?;
    select_value(
        &deduplication,
        "while_unfinished_for_key",
        "produced-work deduplication",
    )
    .await?;
    let deduplication_key =
        find(driver, By::Css("[data-produced-deduplication-key='true']")).await?;
    replace_element_value(
        &deduplication_key,
        "browser-campaign",
        "produced-work deduplication key",
    )
    .await?;
    let add_label = producing
        .find(By::Css("[data-produced-add-label='true']"))
        .await
        .context("failed to find produced-work add-label control")?;
    click_element(&add_label, "add produced-work initial label").await?;
    producing
        .find(By::Css("[data-produced-work-editor='structured']"))
        .await
        .context("failed to find structured produced-work editor")?;
    assert_that!(element_value(&deduplication_key, "produced-work deduplication key").await?)
        .is_equal_to("browser-campaign");
    producing
        .find(By::Css("[data-produced-label='0']"))
        .await
        .context("failed to find produced-work initial label")?;
    Ok(())
}

async fn assert_personality_revision_restore(driver: &WebDriver) -> Result<(), Report> {
    let input = serde_json::json!({
        "key": "browser-personality",
        "name": "Browser personality",
        "description": "Revision one"
    });
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        "/operator/api/projects/demo/automation/personalities",
    )
    .await?
    .json(&input)
    .send()
    .await
    .context("failed to seed personality through operator API")?;
    let status = response.status();
    let created: serde_json::Value = response
        .json()
        .await
        .context("failed to read seeded personality")?;
    if !status.is_success() {
        bail!("failed to seed personality with {status}: {created}");
    }
    let personality_id = created["id"]
        .as_i64()
        .context_with(|| format!("seeded personality response had no id: {created}"))?;
    let response = browser_request(
        driver,
        reqwest::Method::PUT,
        &format!("/operator/api/projects/demo/automation/personalities/{personality_id}"),
    )
    .await?
    .json(&serde_json::json!({
        "key": "browser-personality",
        "name": "Browser personality",
        "description": "Revision two"
    }))
    .send()
    .await
    .context("failed to update seeded personality")?;
    response_text(response, "personality revision update").await?;

    let automation_url = driver
        .current_url()
        .await
        .context("failed to read Automation page URL")?;
    driver
        .goto(automation_url.as_str())
        .await
        .context("failed to reload Automation page after seeding personality revisions")?;
    click(
        driver,
        By::XPath(
            "//*[@id='personalities']//leptonic-table-row[contains(., 'Browser personality')]",
        ),
    )
    .await
    .context("failed to select browser-test personality")?;
    find(
        driver,
        By::Css("[data-testid='automation-personality-inspector']"),
    )
    .await?;
    click(
        driver,
        By::XPath(
            "//*[@data-testid='automation-personality-inspector']//button[normalize-space()='Restore']",
        ),
    )
    .await
    .context("failed to restore browser-test personality revision")?;
    wait_until("restored personality revision", || async {
        let inspectors = driver
            .find_all(By::Css("[data-testid='automation-personality-inspector']"))
            .await
            .context("failed to inspect restored personality revision")?;
        let Some(inspector) = inspectors.first() else {
            return Ok(None);
        };
        let text = inspector
            .text()
            .await
            .context("failed to read restored personality revision")?;
        Ok((text.contains("Revision 3") && text.contains("Current")).then_some(()))
    })
    .await?;
    Ok(())
}

async fn create_trigger(driver: &WebDriver) -> Result<(), Report> {
    let project_id = test_project_id(driver).await?;
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        "/api/automation_triggers/crud/create-one",
    )
    .await?
    .json(&serde_json::json!({
        "entity": {
            "project_id": project_id,
            "name": "refine-new",
            "enabled": true,
            "activation": "work_item_created",
            "effect": "consume_work",
            "schedule": "@every 15s",
            "tool_name": "codex",
            "mutability": "read_only",
            "prompt": "Refine new work items.",
            "work_item_selector": "{\"All\":[{\"column_name\":\"state\",\"operator\":\"=\",\"value\":{\"String\":\"open\"}}]}",
            "priority": 0
        }
    }))
    .send()
    .await
    .context("failed to create trigger through CrudKit browser-test setup request")?;
    response_text(response, "automation-trigger setup").await?;
    Ok(())
}
