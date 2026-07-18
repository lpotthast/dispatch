use std::{borrow::Cow, time::Duration};

use assertr::prelude::*;
use browser_test::thirtyfour::{By, Key, WebDriver, components::SelectElement};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};
use rootcause::option_ext::OptionExt;

use super::common::*;

pub(crate) struct ProjectAdministrationTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for ProjectAdministrationTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("project administration preserves edits and structured lanes")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, true).await?;
        seed_system_prompt_history(driver).await?;
        seed_memory_history(driver).await?;
        driver
            .goto(app.url("/project?project=demo"))
            .await
            .context("failed to open selected-project administration")?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Project");
        find(
            driver,
            By::Css(".top-nav a.active[href='/project?project=demo']"),
        )
        .await?;
        find(driver, By::Css("section.project-settings")).await?;
        assert_source_does_not_contain(driver, "project-workspace").await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='work-items'] .crud-nav"),
        )
        .await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"work-items\"").await?;
        assert_crudkit_create_form_survives_live_event(driver).await?;
        assert_admin_dirty_primary_navigation(driver).await?;
        assert_dirty_project_switch(driver).await?;
        driver
            .goto(app.url("/project?project=demo"))
            .await
            .context("failed to restore selected-project administration after project switch")?;
        find(driver, By::Css("section.project-settings")).await?;
        assert_request_error_toast_preserves_project_page(driver).await?;
        driver
            .goto(app.url("/project?project=demo"))
            .await
            .context("failed to restore selected-project administration after request check")?;
        find(driver, By::Css("section.project-settings")).await?;
        assert_source_contains(driver, "System prompt").await?;
        assert_source_contains(driver, "Memory").await?;
        assert_source_does_not_contain(driver, "Automation policy").await?;
        assert_source_contains(driver, "Maintenance").await?;
        assert_source_contains(driver, "Cleanup worktrees").await?;
        assert_source_does_not_contain(driver, "project-option-key").await?;
        assert_source_contains(driver, "system prompt history").await?;
        assert_source_contains(driver, "memory history").await?;
        assert_source_does_not_contain(driver, "Compact history").await?;
        assert_source_does_not_contain(driver, "Append memory").await?;
        assert_source_does_not_contain(driver, "append-memory").await?;
        assert_source_does_not_contain(driver, "/memory/append").await?;
        assert_source_does_not_contain(driver, "memory-history-entry").await?;
        assert_source_does_not_contain(driver, "memory-snapshot").await?;
        assert_source_does_not_contain(driver, "Allow refinement while editing").await?;
        find(driver, By::Css("#project-system-prompt-version")).await?;
        find(driver, By::Css("textarea.project-system-prompt-text")).await?;
        assert_system_prompt_history_selector_behaviour(driver).await?;
        find(driver, By::Css("#project-memory-version")).await?;
        find(driver, By::Css("textarea.project-memory-text")).await?;
        assert_memory_history_selector_behaviour(driver).await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='work-item-states'] .crud-nav"),
        )
        .await?;
        find(
            driver,
            By::Css("[data-crudkit-leptos='swim-lanes'] .crud-nav"),
        )
        .await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"work-item-states\"").await?;
        assert_source_contains(driver, "data-crudkit-leptos=\"swim-lanes\"").await?;
        assert_swim_lane_create_form_exposes_structured_filter(driver).await?;
        let structured_lane_id = create_swim_lane_filter_seed_lane(driver).await?;
        edit_swim_lane_filter_through_structured_controls(driver, app, structured_lane_id).await?;
        create_structured_lane_matching_item(driver).await?;
        assert_structured_swim_lane_filter_board_behaviour(driver, app).await?;

        Ok(())
    }
}

async fn assert_swim_lane_create_form_exposes_structured_filter(
    driver: &WebDriver,
) -> Result<(), Report> {
    click(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-editor='structured']"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-add-clause='true']"),
    )
    .await?;
    assert_source_does_not_contain(driver, "placeholder=\"{&quot;All&quot;").await?;
    Ok(())
}

async fn create_swim_lane_filter_seed_lane(driver: &WebDriver) -> Result<i64, Report> {
    let project_id = test_project_id(driver).await?;
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        "/api/swim_lanes/crud/create-one",
    )
    .await?
    .json(&serde_json::json!({
        "entity": {
            "project_id": project_id,
            "identifier": "filtered",
            "name": "Filtered",
            "position": 45,
            "filter": "{\"All\":[]}",
            "item_order": "updated_desc",
            "can_create_items": true
        }
    }))
    .send()
    .await
    .context("failed to create swim-lane through CrudKit browser-test setup request")?;
    let status = response.status();
    let saved: serde_json::Value = response
        .json()
        .await
        .context("failed to read swim-lane setup response")?;
    if !status.is_success() {
        bail!("failed to create swim-lane through CrudKit: {status}: {saved}");
    }
    Ok(saved["entity"]["id"]
        .as_i64()
        .context_with(|| format!("created swim-lane response did not contain an id: {saved}"))?)
}

async fn edit_swim_lane_filter_through_structured_controls(
    driver: &WebDriver,
    app: &DispatchTestApp,
    lane_id: i64,
) -> Result<(), Report> {
    driver
        .goto(app.url(&format!(
            "/project?project=demo&edit_swim_lane={lane_id}#swim-lanes"
        )))
        .await
        .context("failed to open swim-lane edit form")?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='swim-lanes'] [data-lane-filter-editor='structured']"),
    )
    .await?;

    let root_group = find(driver, By::Css("[data-lane-filter-group='root']")).await?;
    let root_add_clause = root_group
        .find(By::Css("[data-lane-filter-add-clause='true']"))
        .await
        .context("failed to find root filter add-clause button")?;
    click_element(&root_add_clause, "root filter add-clause button").await?;
    let root_clause = root_group
        .find(By::Css(
            ":scope > .lane-filter-elements > .lane-filter-clause:nth-child(1)",
        ))
        .await
        .context("failed to find root filter clause")?;
    replace_element_value(
        &root_clause
            .find(By::Css("[data-lane-filter-key='true']"))
            .await
            .context("failed to find root filter key")?,
        "state",
        "root filter key",
    )
    .await?;
    replace_element_value(
        &root_clause
            .find(By::Css("[data-lane-filter-value='true']"))
            .await
            .context("failed to find root filter value")?,
        "open",
        "root filter value",
    )
    .await?;

    let root_add_group = root_group
        .find(By::Css("[data-lane-filter-add-group='true']"))
        .await
        .context("failed to find nested-group button")?;
    click_element(&root_add_group, "nested filter group button").await?;
    let nested_group = find(driver, By::Css("[data-lane-filter-group='1']")).await?;
    let nested_kind = nested_group
        .find(By::Css(".lane-filter-group-kind select"))
        .await
        .context("failed to find nested filter group kind")?;
    select_value(&nested_kind, "any", "nested filter group kind").await?;

    let first_nested_add_clause = nested_group
        .find(By::Css("[data-lane-filter-add-clause='true']"))
        .await
        .context("failed to find nested filter add-clause button")?;
    click_element(
        &first_nested_add_clause,
        "first nested filter add-clause button",
    )
    .await?;
    let first_nested_clause = nested_group
        .find(By::Css(
            ":scope > .lane-filter-elements > .lane-filter-clause:nth-child(1)",
        ))
        .await
        .context("failed to find first nested filter clause")?;
    replace_element_value(
        &first_nested_clause
            .find(By::Css("[data-lane-filter-key='true']"))
            .await
            .context("failed to find nested severity key")?,
        "severity",
        "nested severity key",
    )
    .await?;
    let severity_operator = first_nested_clause
        .find(By::Css("[data-lane-filter-operator='true']"))
        .await
        .context("failed to find nested severity operator")?;
    select_value(&severity_operator, "is_in", "nested severity operator").await?;
    replace_element_value(
        &find(driver, By::Css("[data-lane-filter-value-list='true']")).await?,
        "critical, high",
        "nested severity values",
    )
    .await?;

    let second_nested_add_clause = nested_group
        .find(By::Css("[data-lane-filter-add-clause='true']"))
        .await
        .context("failed to find second nested filter add-clause button")?;
    click_element(
        &second_nested_add_clause,
        "second nested filter add-clause button",
    )
    .await?;
    let second_nested_clause = nested_group
        .find(By::Css(
            ":scope > .lane-filter-elements > .lane-filter-clause:nth-child(2)",
        ))
        .await
        .context("failed to find second nested filter clause")?;
    replace_element_value(
        &second_nested_clause
            .find(By::Css("[data-lane-filter-key='true']"))
            .await
            .context("failed to find presence-filter key")?,
        "needs-verification",
        "presence-filter key",
    )
    .await?;
    let presence_operator = second_nested_clause
        .find(By::Css("[data-lane-filter-operator='true']"))
        .await
        .context("failed to find presence-filter operator")?;
    select_value(&presence_operator, "present", "presence-filter operator").await?;

    click(driver, By::Css(".lane-filter-raw-toggle")).await?;
    let raw = element_value(
        &find(driver, By::Css("[data-lane-filter-raw='true']")).await?,
        "raw swim-lane filter",
    )
    .await?;
    assert_that!(raw.clone()).contains("\"Any\"");
    assert_that!(raw.clone()).contains("\"severity\"");
    assert_that!(raw).contains("\"is_in\"");
    click(driver, By::Css(".lane-filter-structured-toggle")).await?;
    click(
        driver,
        By::XPath(
            "//div[@data-crudkit-leptos='swim-lanes']//button[normalize-space()='Speichern']",
        ),
    )
    .await?;
    wait_for_structured_swim_lane_filter_saved(driver, lane_id).await?;
    Ok(())
}

async fn wait_for_structured_swim_lane_filter_saved(
    driver: &WebDriver,
    lane_id: i64,
) -> Result<(), Report> {
    wait_until("saved structured swim-lane filter", || async {
        let response = browser_request(
            driver,
            reqwest::Method::POST,
            "/api/swim_lanes/crud/read-many",
        )
        .await?
        .json(&serde_json::json!({
            "limit": 1,
            "skip": null,
            "order_by": null,
            "condition": {
                "All": [{
                    "column_name": "id",
                    "operator": "=",
                    "value": { "I64": lane_id }
                }]
            }
        }))
        .send()
        .await
        .context("failed to read saved structured swim-lane filter")?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let rows: serde_json::Value = response
            .json()
            .await
            .context("failed to decode saved structured swim-lane filter")?;
        let filter = rows
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|lane| lane["filter"].as_str())
            .unwrap_or_default();
        Ok((filter.contains("\"Any\"")
            && filter.contains("\"state\"")
            && filter.contains("\"severity\"")
            && filter.contains("\"needs-verification\""))
        .then_some(()))
    })
    .await?;
    Ok(())
}

async fn create_structured_lane_matching_item(driver: &WebDriver) -> Result<(), Report> {
    let response = browser_request(driver, reqwest::Method::POST, "/api/projects/demo/items")
        .await?
        .json(&serde_json::json!({
            "title": "Structured lane item",
            "description": "Created for the structured swim-lane filter browser test",
            "state": "open",
            "initial_labels": [{ "key": "severity", "value": "high" }],
            "agent_model_override": null,
            "agent_reasoning_effort_override": null
        }))
        .send()
        .await
        .context("failed to create structured-lane matching item")?;
    response_text(response, "structured-lane item creation").await?;
    Ok(())
}

async fn assert_structured_swim_lane_filter_board_behaviour(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    clear_board_service_cache(driver).await?;
    driver
        .goto(app.url("/?project=demo"))
        .await
        .context("failed to open board after structured swim-lane edit")?;
    find(
        driver,
        By::XPath("//section[contains(@class, 'lane')]//h2[.='Filtered']"),
    )
    .await?;
    let lane = find(
        driver,
        By::XPath("//section[contains(@class, 'lane')][.//h2[normalize-space()='Filtered']]"),
    )
    .await?;
    assert_that!(
        lane.text()
            .await
            .context("failed to read structured swim-lane contents")?
    )
    .contains("Structured lane item");
    let lane_add = lane
        .find(By::Css(".lane-add"))
        .await
        .context("failed to find structured swim-lane add button")?;
    click_element(&lane_add, "structured swim-lane add button").await?;
    find(driver, By::Css("#new-item-modal select[name='state']")).await?;
    let state_select = find(driver, By::Css("#new-item-modal select[name='state']")).await?;
    let options = SelectElement::new(&state_select)
        .await
        .context("failed to inspect structured swim-lane state select")?
        .options()
        .await
        .context("failed to read structured swim-lane state options")?;
    let mut option_values = Vec::with_capacity(options.len());
    for option in options {
        option_values.push(
            option
                .attr("value")
                .await
                .context("failed to read structured swim-lane state option")?
                .unwrap_or_default(),
        );
    }
    let state = format!(
        "{}|{}",
        element_value(&state_select, "structured swim-lane state").await?,
        option_values.join(",")
    );
    assert_that!(state).is_equal_to("open|open".to_owned());
    close_clean_new_item_modal(driver).await?;
    Ok(())
}

async fn seed_memory_history(driver: &WebDriver) -> Result<(), Report> {
    for body in ["Initial shared memory", "Current shared memory"] {
        let response = browser_request(driver, reqwest::Method::POST, "/projects/demo/memory")
            .await?
            .form(&[("body", body)])
            .send()
            .await
            .context("failed to seed project memory through browser-test setup request")?;
        response_text(response, "project-memory seed").await?;
    }
    Ok(())
}

async fn seed_system_prompt_history(driver: &WebDriver) -> Result<(), Report> {
    for body in ["Initial project prompt", "Current project prompt"] {
        let response = browser_request(
            driver,
            reqwest::Method::POST,
            "/projects/demo/system-prompt",
        )
        .await?
        .form(&[("body", body)])
        .send()
        .await
        .context("failed to seed project system prompt through browser-test setup request")?;
        response_text(response, "project-system-prompt seed").await?;
    }
    Ok(())
}

async fn assert_memory_history_selector_behaviour(driver: &WebDriver) -> Result<(), Report> {
    assert_history_selector_behaviour(
        driver,
        "#project-memory-version",
        "textarea.project-memory-text",
        ".project-memory-editor button",
        "Unsaved current memory",
        "Initial shared memory",
        "memory",
    )
    .await
}

async fn assert_system_prompt_history_selector_behaviour(driver: &WebDriver) -> Result<(), Report> {
    assert_history_selector_behaviour(
        driver,
        "#project-system-prompt-version",
        "textarea.project-system-prompt-text",
        ".project-system-prompt-editor button",
        "Unsaved current prompt",
        "Initial project prompt",
        "system prompt",
    )
    .await
}

async fn assert_crudkit_create_form_survives_live_event(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-input-field"),
    )
    .await?;

    set_input_value(
        driver,
        "[data-crudkit-leptos='work-items'] .crud-input-field",
        "Draft survives live event",
    )
    .await?;
    create_browser_test_item(
        driver,
        "Live refresh item",
        "Created to emit a websocket event",
    )
    .await?;
    wait_until("CrudKit draft to survive live event", || async {
        let input = editable_element(
            &find(
                driver,
                By::Css("[data-crudkit-leptos='work-items'] .crud-input-field"),
            )
            .await?,
        )
        .await?;
        let draft_survived =
            element_value(&input, "work-item create draft").await? == "Draft survives live event";
        let workspace_present = !driver
            .find_all(By::Css(".workspace-dock .workspace-actions"))
            .await
            .context("failed to inspect workspace after live event")?
            .is_empty();
        Ok((draft_survived && workspace_present).then_some(()))
    })
    .await?;
    Ok(())
}

async fn assert_admin_dirty_primary_navigation(driver: &WebDriver) -> Result<(), Report> {
    let expected_draft = "Draft guards primary navigation";
    set_input_value(
        driver,
        "[data-crudkit-leptos='work-items'] .crud-input-field",
        expected_draft,
    )
    .await?;
    let project_url = driver
        .current_url()
        .await
        .context("failed to read dirty project administration URL")?;

    click(driver, By::Css(".top-nav a[href='/runs?project=demo']")).await?;
    find_leave_modal(driver)
        .await
        .context("dirty admin editor did not guard primary navigation")?;
    click_leave_modal_cancel(driver).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read project URL after cancelling primary navigation")?
    )
    .is_equal_to(project_url);
    assert_work_item_create_title_value(driver, expected_draft).await?;

    assert_modified_topbar_click_is_not_intercepted(driver).await?;

    click(
        driver,
        By::XPath("//*[@data-crudkit-leptos='work-items']//button[contains(., 'Zurück')]"),
    )
    .await?;
    find_leave_modal(driver)
        .await
        .context("dirty admin editor return did not open the leave guard")?;
    click_leave_modal_accept(driver).await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-nav"),
    )
    .await?;
    Ok(())
}

async fn assert_modified_topbar_click_is_not_intercepted(driver: &WebDriver) -> Result<(), Report> {
    let original_window = driver
        .window()
        .await
        .context("failed to read original window before modified top-bar click")?;
    let original_windows = driver
        .windows()
        .await
        .context("failed to inspect windows before modified top-bar click")?;
    let link = find(driver, By::Css(".top-nav a[href='/runs?project=demo']")).await?;
    driver
        .action_chain()
        .key_down(Key::Meta)
        .click_element(&link)
        .key_up(Key::Meta)
        .perform()
        .await
        .context("failed to perform modified top-bar click")?;
    let new_window = wait_until("modified click to open a native browser tab", || async {
        let windows = driver
            .windows()
            .await
            .context("failed to inspect windows after modified top-bar click")?;
        Ok(windows
            .into_iter()
            .find(|window| !original_windows.contains(window)))
    })
    .await?;
    driver
        .switch_to_window(new_window)
        .await
        .context("failed to switch to modified-click tab")?;
    find(
        driver,
        By::Css(".top-nav a.active[href='/runs?project=demo']"),
    )
    .await
    .context("modified-click tab did not finish rendering the Runs page")?;
    driver
        .close_window()
        .await
        .context("failed to close modified-click tab")?;
    driver
        .switch_to_window(original_window)
        .await
        .context("failed to restore original browser-test tab")?;
    Ok(())
}

async fn assert_dirty_project_switch(driver: &WebDriver) -> Result<(), Report> {
    click(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-nav button"),
    )
    .await?;
    find(
        driver,
        By::Css("[data-crudkit-leptos='work-items'] .crud-input-field"),
    )
    .await?;

    let expected_draft = "Draft guards project switch";
    set_input_value(
        driver,
        "[data-crudkit-leptos='work-items'] .crud-input-field",
        expected_draft,
    )
    .await?;
    let original_url = driver
        .current_url()
        .await
        .context("failed to read Project page URL before project switch")?;
    let original_selection = project_switcher_selection(driver).await?;

    choose_project_in_switcher(driver, "Demo Alt").await?;
    find_leave_modal(driver)
        .await
        .context("dirty admin editor did not guard project switch")?;
    click_leave_modal_cancel(driver).await?;

    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read Project page URL after cancelling project switch")?
    )
    .is_equal_to(original_url);
    assert_that!(project_switcher_selection(driver).await?).is_equal_to(original_selection);
    assert_work_item_create_title_value(driver, expected_draft).await?;

    choose_project_in_switcher(driver, "Demo Alt").await?;
    find_leave_modal(driver)
        .await
        .context("dirty admin editor did not guard accepted project switch")?;
    let history_length_before_accept = browser_history_entry_count(driver).await?;
    click_leave_modal_accept(driver).await?;

    wait_until("accepted project switch", || async {
        let current_url = driver
            .current_url()
            .await
            .context("failed to read Project page URL after accepting project switch")?;
        let selected_project = project_switcher_selection(driver).await?;
        let switched = current_url.path() == "/project"
            && current_url
                .query_pairs()
                .any(|(key, value)| key == "project" && value == "demo-alt")
            && selected_project.contains("Demo Alt");
        Ok(switched.then_some(()))
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_that!(browser_history_entry_count(driver).await?)
        .is_equal_to(history_length_before_accept + 1);
    Ok(())
}

async fn choose_project_in_switcher(
    driver: &WebDriver,
    project_display_name: &str,
) -> Result<(), Report> {
    click(
        driver,
        By::Css(".project-switcher leptonic-select-selected"),
    )
    .await?;
    click(
        driver,
        By::XPath(format!(
            "//div[contains(@class, 'project-switcher')]//leptonic-select-option[contains(., '{project_display_name}')]"
        )),
    )
    .await
}

async fn project_switcher_selection(driver: &WebDriver) -> Result<String, Report> {
    Ok(find(
        driver,
        By::Css(".project-switcher leptonic-select-selected"),
    )
    .await?
    .text()
    .await
    .context("failed to read displayed project-switcher selection")?)
}

async fn browser_history_entry_count(driver: &WebDriver) -> Result<usize, Report> {
    Ok(driver
        .cdp()
        .send_raw("Page.getNavigationHistory", ())
        .await
        .context("failed to read browser navigation history")?["entries"]
        .as_array()
        .context("browser navigation history did not contain entries")?
        .len())
}

async fn assert_request_error_toast_preserves_project_page(
    driver: &WebDriver,
) -> Result<(), Report> {
    driver
        .cdp()
        .send_raw("Network.enable", serde_json::json!({}))
        .await
        .context("failed to enable browser network controls")?;
    let project_page_url_pattern = format!(
        "{}/leptos/load_project_page*",
        driver
            .current_url()
            .await
            .context("failed to read Project page URL for request blocking")?
            .origin()
            .ascii_serialization()
    );
    driver
        .cdp()
        .send_raw(
            "Network.setBlockedURLs",
            serde_json::json!({
                "urlPatterns": [{
                    "urlPattern": project_page_url_pattern,
                    "block": true
                }]
            }),
        )
        .await
        .context("failed to block the Project page request")?;

    click(
        driver,
        By::Css(".project-switcher leptonic-select-selected"),
    )
    .await?;
    click(
        driver,
        By::XPath(
            "//div[contains(@class, 'project-switcher')]//leptonic-select-option[contains(., 'Demo Alt')]",
        ),
    )
    .await?;

    let assertion = wait_until("failed page request error toast", || async {
        let toasts = driver
            .find_all(By::Css("leptonic-toast[data-variant='error']"))
            .await
            .context("failed to inspect page-request error toast")?;
        let mut has_error_toast = false;
        for toast in toasts {
            if toast
                .text()
                .await
                .context("failed to read page-request error toast")?
                .contains("Request failed")
            {
                has_error_toast = true;
                break;
            }
        }
        Ok(has_error_toast.then_some(()))
    })
    .await;
    driver
        .cdp()
        .send_raw(
            "Network.setBlockedURLs",
            serde_json::json!({ "urlPatterns": [], "urls": [] }),
        )
        .await
        .context("failed to restore Project page requests")?;
    assertion?;
    find(driver, By::Css("header.app-topbar .brand")).await?;
    assert_that!(
        driver
            .find_all(By::Css("header.app-topbar .top-nav a"))
            .await
            .context("failed to inspect top navigation after request failure")?
            .len()
    )
    .is_equal_to(7);
    for selector in [
        "header.app-topbar .project-switcher",
        "header.app-topbar .topbar-codex",
        "header.app-topbar .topbar-automation",
        "main.page-shell",
        "main.project-page",
    ] {
        find(driver, By::Css(selector)).await?;
    }
    assert_source_does_not_contain(driver, ">Loading...</").await?;

    Ok(())
}

async fn assert_history_selector_behaviour(
    driver: &WebDriver,
    select_selector: &str,
    textarea_selector: &str,
    save_selector: &str,
    current_draft: &str,
    historical_value: &str,
    description: &str,
) -> Result<(), Report> {
    let textarea = find(driver, By::Css(textarea_selector)).await?;
    replace_element_value(&textarea, current_draft, description).await?;
    wait_until(&format!("{description} editor hydration"), || async {
        let class = find(driver, By::Css(textarea_selector))
            .await?
            .class_name()
            .await
            .context_with(|| format!("failed to inspect {description} draft state"))?
            .unwrap_or_default();
        Ok(class
            .split_whitespace()
            .any(|class| class == "dirty")
            .then_some(()))
    })
    .await?;

    let select = find(driver, By::Css(select_selector)).await?;
    assert_that!(element_value(&select, description).await?).is_equal_to("current");
    let select_component = SelectElement::new(&select)
        .await
        .context_with(|| format!("failed to inspect {description} history selector"))?;
    let options = select_component
        .options()
        .await
        .context_with(|| format!("failed to read {description} history options"))?;
    if options.len() < 3 {
        bail!(
            "expected current plus history options for {description}, got {}",
            options.len()
        );
    }
    assert_that!(element_value(&textarea, description).await?).is_equal_to(current_draft);
    let historical_option = options[2]
        .attr("value")
        .await
        .context_with(|| format!("failed to read {description} historical option"))?
        .context_with(|| format!("{description} historical option had no value"))?;
    select_component
        .select_by_value(&historical_option)
        .await
        .context_with(|| format!("failed to select historical {description}"))?;
    wait_until(&format!("historical {description}"), || async {
        let textarea = find(driver, By::Css(textarea_selector)).await?;
        Ok((element_value(&textarea, description).await? == historical_value).then_some(()))
    })
    .await?;
    let historical_textarea = find(driver, By::Css(textarea_selector)).await?;
    assert_that!(
        historical_textarea
            .attr("readonly")
            .await
            .context_with(|| format!("failed to inspect historical {description}"))?
            .is_some()
    )
    .is_true();
    assert_that!(
        historical_textarea
            .class_name()
            .await
            .context_with(|| format!("failed to inspect historical {description} classes"))?
            .unwrap_or_default()
            .split_whitespace()
            .any(|class| class == "dirty")
    )
    .is_false();
    assert_that!(
        find(driver, By::Css(save_selector))
            .await?
            .is_enabled()
            .await
            .context_with(|| format!("failed to inspect historical {description} save action"))?
    )
    .is_false();

    let select = find(driver, By::Css(select_selector)).await?;
    select_value(&select, "current", description).await?;
    wait_until(&format!("current {description} draft"), || async {
        let textarea = find(driver, By::Css(textarea_selector)).await?;
        Ok((element_value(&textarea, description).await? == current_draft).then_some(()))
    })
    .await?;
    let current_textarea = find(driver, By::Css(textarea_selector)).await?;
    assert_that!(
        current_textarea
            .attr("readonly")
            .await
            .context_with(|| format!("failed to inspect current {description}"))?
            .is_none()
    )
    .is_true();
    assert_that!(
        current_textarea
            .class_name()
            .await
            .context_with(|| format!("failed to inspect current {description} classes"))?
            .unwrap_or_default()
            .split_whitespace()
            .any(|class| class == "dirty")
    )
    .is_true();
    assert_that!(
        find(driver, By::Css(save_selector))
            .await?
            .is_enabled()
            .await
            .context_with(|| format!("failed to inspect current {description} save action"))?
    )
    .is_true();
    Ok(())
}
