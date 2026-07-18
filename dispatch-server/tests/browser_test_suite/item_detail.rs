use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver, components::SelectElement};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};

use super::common::*;

pub(crate) struct ItemDetailTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for ItemDetailTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("item detail supports comments claims and labels")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, true).await?;
        seed_run_commit_outcome_fixtures(app).await?;
        create_browser_test_item(driver, "Browser item", "Created through browser-test").await?;
        link_run_fixtures_to_browser_item(app).await?;
        driver
            .goto(app.url("/?project=demo"))
            .await
            .context("failed to open Board for item-detail fixture")?;
        open_full_browser_item_from_board(driver).await?;
        find(driver, By::Css("section.item-settings")).await?;
        assert_item_detail_dirty_leave_protection(driver).await?;
        open_full_browser_item_from_board(driver).await?;
        find(driver, By::Css("section.item-settings")).await?;
        assert_source_does_not_contain(driver, "automation can claim this item").await?;
        assert_source_does_not_contain(driver, "Set state").await?;
        find(
            driver,
            By::XPath(
                "//section[contains(@class, 'item-settings')]//button[contains(., 'Löschen')]",
            ),
        )
        .await?;
        assert_source_does_not_contain(driver, "Start agent").await?;
        assert_source_contains(driver, "Comments").await?;
        assert_user_comment_create_flow(driver).await?;
        add_agent_comment(driver).await?;
        claim_current_item(driver).await?;
        activate_browser_item_run(app).await?;
        let item_url = driver
            .current_url()
            .await
            .context("failed to read item URL after adding agent comment")?;
        driver
            .goto(item_url.as_str())
            .await
            .context("failed to reload item page after adding agent comment")?;
        find(
            driver,
            By::Css(
                "section.comments .comment-author-link[href='/projects/demo/automation/runs/60/log']",
            ),
        )
        .await?;
        find(
            driver,
            By::Css(".item-meta a.claim-badge[href='/projects/demo/automation/runs/503/log']"),
        )
        .await?;
        assert_source_contains(driver, "dispatch-run-503").await?;
        assert_board_active_run_context(driver, app, item_url.as_str()).await?;
        find(driver, By::Css("section.item-labels")).await?;
        assert_state_label_dropdown_and_move(driver).await?;
        send_keys(
            driver,
            By::Css(".label-add-controls input[name='key']"),
            "severity",
        )
        .await?;
        send_keys(
            driver,
            By::Css(".label-add-controls input[name='value']"),
            "high",
        )
        .await?;
        submit_label_add_form(driver).await?;
        find(driver, By::Css(".label-row[data-label-key='severity']")).await?;
        assert_label_add_preserved_item_page(driver).await?;
        assert_item_label_update_delete_flow(driver).await?;

        Ok(())
    }
}

async fn assert_user_comment_create_flow(driver: &WebDriver) -> Result<(), Report> {
    let original_url = driver
        .current_url()
        .await
        .context("failed to read item URL before adding user comment")?;
    replace_element_value(
        &find(
            driver,
            By::Css("section.comments input[name='author_name']"),
        )
        .await?,
        "Browser user",
        "comment author",
    )
    .await?;
    replace_element_value(
        &find(driver, By::Css("section.comments textarea[name='body']")).await?,
        "Typed browser comment",
        "comment body",
    )
    .await?;
    click(
        driver,
        By::Css("section.comments .comment-add-controls button"),
    )
    .await?;
    find(
        driver,
        By::XPath(
            "//section[contains(@class, 'comments')]//p[normalize-space()='Typed browser comment']",
        ),
    )
    .await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read item URL after adding user comment")?
    )
    .is_equal_to(original_url);
    assert_that!(
        element_value(
            &find(
                driver,
                By::Css("section.comments input[name='author_name']"),
            )
            .await?,
            "reset comment author",
        )
        .await?
    )
    .is_empty();
    assert_that!(
        element_value(
            &find(driver, By::Css("section.comments textarea[name='body']")).await?,
            "reset comment body",
        )
        .await?
    )
    .is_empty();
    Ok(())
}

async fn add_agent_comment(driver: &WebDriver) -> Result<(), Report> {
    let item_id = current_item_id(driver).await?;
    let response = browser_request(
        driver,
        reqwest::Method::POST,
        &format!("/api/projects/demo/items/{item_id}/comments"),
    )
    .await?
    .json(&serde_json::json!({
        "author_type": "agent",
        "author_name": "dispatch-run-60",
        "body": "Agent progress from browser test"
    }))
    .send()
    .await
    .context("failed to add agent comment through API browser-test request")?;
    response_text(response, "agent-comment setup")
        .await
        .context("failed to read agent comment setup response")?;
    Ok(())
}

async fn claim_current_item(driver: &WebDriver) -> Result<(), Report> {
    let item_id = current_item_id(driver).await?;
    let move_response = browser_request(
        driver,
        reqwest::Method::PATCH,
        &format!("/api/projects/demo/items/{item_id}"),
    )
    .await?
    .json(&serde_json::json!({ "state": "browser-claimable" }))
    .send()
    .await
    .context("failed to make current item claimable")?;
    response_text(move_response, "claimable-state update").await?;

    let claim_response = browser_request(
        driver,
        reqwest::Method::POST,
        "/api/projects/demo/items/claim",
    )
    .await?
    .json(&serde_json::json!({
        "agent_id": "dispatch-run-503",
        "state": "browser-claimable"
    }))
    .send()
    .await
    .context("failed to claim current item through API browser-test request")?;
    let status = claim_response.status();
    let payload: serde_json::Value = claim_response
        .json()
        .await
        .context("failed to read item claim setup response")?;
    if !status.is_success() {
        bail!("item claim failed with {status}: {payload}");
    }
    assert_that!(payload["item"]["id"].as_i64()).is_equal_to(Some(item_id));
    Ok(())
}

async fn assert_board_active_run_context(
    driver: &WebDriver,
    app: &DispatchTestApp,
    item_url: &str,
) -> Result<(), Report> {
    clear_board_service_cache(driver).await?;
    driver
        .goto(app.url("/?project=demo"))
        .await
        .context("failed to open Board for active run context assertion")?;
    let card = find(
        driver,
        By::XPath(
            "//article[contains(@class, 'card')][.//*[contains(@class, 'card-main-link')]//h3[normalize-space()='Browser item']]",
        ),
    )
    .await?;
    let overview = card
        .find(By::Css(".card-runs"))
        .await
        .context("failed to find Board run overview")?;
    let run = card
        .find(By::Css(".card-run-preview[href$='/runs/503/log']"))
        .await
        .context("failed to find active Board run preview")?;
    assert_that!(
        card.find_all(By::Css(".claim-badge"))
            .await
            .context("failed to inspect Board claim badge")?
            .is_empty()
    )
    .is_true();
    assert_that!(
        run.find(By::Css(".card-run-source"))
            .await
            .context("failed to find Board run source")?
            .text()
            .await
            .context("failed to read Board run source")?
    )
    .is_equal_to("via Claim open work");
    let elapsed = run
        .find(By::Css(".claim-elapsed"))
        .await
        .context("failed to find Board run elapsed time")?
        .text()
        .await
        .context("failed to read Board run elapsed time")?;
    assert_that!(is_elapsed_time(&elapsed)).is_true();
    for property in ["margin-top", "margin-right", "margin-bottom", "margin-left"] {
        let value = overview
            .css_value(property)
            .await
            .context_with(|| format!("failed to read Board run overview {property}"))?;
        let pixels = value
            .strip_suffix("px")
            .unwrap_or(&value)
            .parse::<f64>()
            .context_with(|| format!("failed to parse Board run overview {property}: {value:?}"))?;
        assert_that!(pixels > 0.0).is_true();
    }

    driver
        .goto(item_url)
        .await
        .context("failed to return to item after active run context assertion")?;
    find(driver, By::Css("section.item-settings")).await?;
    Ok(())
}

async fn assert_item_detail_dirty_leave_protection(driver: &WebDriver) -> Result<(), Report> {
    set_input_value(
        driver,
        "section.item-settings .crud-input-field",
        "Unsaved detail title",
    )
    .await?;
    let item_url = driver
        .current_url()
        .await
        .context("failed to read dirty item URL")?;

    click(driver, By::Css("button.item-board-link")).await?;
    find_leave_modal(driver)
        .await
        .context("full item Board action did not open the dirty-leave guard")?;
    click_leave_modal_cancel(driver).await?;
    assert_item_detail_title_value(driver, "Unsaved detail title").await?;

    click(driver, By::Css(".top-nav a[href='/project?project=demo']")).await?;
    find_leave_modal(driver)
        .await
        .context("dirty item editor did not guard primary navigation")?;
    click_leave_modal_cancel(driver).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read item URL after cancelling primary navigation")?
    )
    .is_equal_to(item_url.clone());
    assert_item_detail_title_value(driver, "Unsaved detail title").await?;

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
    find_leave_modal(driver)
        .await
        .context("dirty item editor did not guard project switch")?;
    click_leave_modal_cancel(driver).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read item URL after cancelling project switch")?
    )
    .is_equal_to(item_url);
    let selected_project = find(
        driver,
        By::Css(".project-switcher leptonic-select-selected"),
    )
    .await?
    .text()
    .await
    .context("failed to read restored project-switcher selection")?;
    if !selected_project.contains("Demo") || selected_project.contains("Demo Alt") {
        bail!("project switcher did not restore Demo after cancellation: {selected_project}");
    }
    assert_item_detail_title_value(driver, "Unsaved detail title").await?;

    click(driver, By::Css(".top-nav a[href='/?project=demo']")).await?;
    find_leave_modal(driver)
        .await
        .context("accepted primary navigation did not open the dirty-leave guard")?;
    click_leave_modal_accept(driver).await?;
    find(driver, By::Css("section.board")).await?;
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

async fn open_browser_item_drawer(driver: &WebDriver) -> Result<(), Report> {
    let item = find_browser_item_card_link(driver).await?;
    click_element(&item, "Browser item card").await?;
    find(
        driver,
        By::Css("leptonic-drawer[data-board-inspector].shown section.item-settings"),
    )
    .await?;
    Ok(())
}

async fn open_full_browser_item_from_board(driver: &WebDriver) -> Result<(), Report> {
    open_browser_item_drawer(driver).await?;
    click(driver, By::Css(".board-drawer-title")).await?;
    find(
        driver,
        By::Css("main.page-shell.item-page section.item-settings"),
    )
    .await?;
    Ok(())
}

async fn submit_label_add_form(driver: &WebDriver) -> Result<(), Report> {
    click(driver, By::Css(".label-add-controls button")).await
}

async fn assert_label_add_preserved_item_page(driver: &WebDriver) -> Result<(), Report> {
    current_item_id(driver).await?;
    find(driver, By::Css("section.item-settings")).await?;
    Ok(())
}

async fn assert_item_label_update_delete_flow(driver: &WebDriver) -> Result<(), Report> {
    let version_before_update = item_detail_version(driver).await?;
    let original_url = driver
        .current_url()
        .await
        .context("failed to read URL before label update")?;
    replace_element_value(
        &find(
            driver,
            By::Css(".label-row[data-label-key='severity'] input[name='value']"),
        )
        .await?,
        "critical",
        "severity label value",
    )
    .await?;
    click(
        driver,
        By::Css(
            ".label-row[data-label-key='severity'] .label-row-actions button:not(.label-delete-button)",
        ),
    )
    .await?;
    wait_for_label_value(driver, "severity", "critical", &version_before_update).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read URL after label update")?
    )
    .is_equal_to(original_url);

    click(
        driver,
        By::Css(".label-row[data-label-key='severity'] .label-delete-button"),
    )
    .await?;
    wait_until("typed label deletion", || async {
        let deleted = driver
            .find_all(By::Css(".label-row[data-label-key='severity']"))
            .await
            .context("failed to inspect typed label deletion")?
            .is_empty();
        Ok(deleted.then_some(()))
    })
    .await?;
    Ok(())
}

async fn wait_for_label_value(
    driver: &WebDriver,
    key: &str,
    expected_value: &str,
    previous_version: &str,
) -> Result<(), Report> {
    wait_until("item label value", || async {
        let field = find(
            driver,
            By::Css(format!(".label-row[data-label-key='{key}'] [name='value']")),
        )
        .await?;
        let current_version = item_detail_version(driver).await?;
        Ok(
            (element_value(&field, "item label value").await? == expected_value
                && current_version != previous_version)
                .then_some(()),
        )
    })
    .await?;
    Ok(())
}

async fn item_detail_version(driver: &WebDriver) -> Result<String, Report> {
    Ok(find(driver, By::Css(".item-meta > span:nth-child(2)"))
        .await?
        .text()
        .await
        .context("failed to read item-detail version")?)
}

async fn assert_state_label_dropdown_and_move(driver: &WebDriver) -> Result<(), Report> {
    let controls = find(driver, By::Css(".label-row .state-label-controls")).await?;
    assert_that!(
        controls
            .find_all(By::Css("input[name='value']"))
            .await
            .context("failed to inspect state label text input")?
            .is_empty()
    )
    .is_true();
    let value_select = controls
        .find(By::Css("select[name='value']"))
        .await
        .context("failed to find state label select")?;
    let options = SelectElement::new(&value_select)
        .await
        .context("failed to inspect state label select")?
        .options()
        .await
        .context("failed to read state label options")?;
    let mut option_summaries = Vec::with_capacity(options.len());
    for option in options {
        option_summaries.push(format!(
            "{}:{}",
            option
                .attr("value")
                .await
                .context("failed to read state option value")?
                .unwrap_or_default(),
            option
                .text()
                .await
                .context("failed to read state option label")?,
        ));
    }
    let summary = format!(
        "value={};hasValueInput=false;options={}",
        element_value(&value_select, "state label select").await?,
        option_summaries.join("|")
    );
    assert_that!(summary).is_equal_to(
        "value=in_progress;hasValueInput=false;options=idea:Idea|open:Open|in_progress:In progress|done:Done"
            .to_owned(),
    );

    let version_before_move = item_detail_version(driver).await?;
    let original_url = driver
        .current_url()
        .await
        .context("failed to read URL before state label move")?;
    select_value(&value_select, "done", "state label").await?;
    click(
        driver,
        By::Css(
            ".label-row .state-label-controls .label-row-actions button:not(.label-delete-button)",
        ),
    )
    .await?;
    wait_for_label_value(driver, "state", "done", &version_before_move).await?;
    assert_that!(
        driver
            .current_url()
            .await
            .context("failed to read URL after state label move")?
    )
    .is_equal_to(original_url);
    Ok(())
}

fn is_elapsed_time(value: &str) -> bool {
    let parts = value.split(':').collect::<Vec<_>>();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|part| part.parse::<u64>().is_ok())
        && parts.iter().skip(1).all(|part| part.len() == 2)
}
