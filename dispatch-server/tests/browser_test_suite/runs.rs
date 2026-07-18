use std::borrow::Cow;

use assertr::prelude::*;
use browser_test::thirtyfour::{By, WebDriver};
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::{Report, ResultExt, bail};

use super::common::*;

pub(crate) struct RunLogsTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for RunLogsTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("run logs render output and commit outcomes")
    }

    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        reset_test_projects(driver, app, false).await?;
        driver
            .goto(app.url("/runs?project=demo"))
            .await
            .context("failed to open Dispatch runs page")?;
        assert_that!(driver.title().await.context("failed to read page title")?)
            .is_equal_to("Runs");
        find(driver, By::Css(".runs-page .automation")).await?;
        find(
            driver,
            By::Css(".top-nav a.active[href='/runs?project=demo']"),
        )
        .await?;
        assert_source_contains(driver, "No runs yet").await?;
        assert_source_contains(driver, "0 running (0 mutating, 0 read-only)").await?;
        assert_source_does_not_contain(driver, "data-crudkit-leptos=\"automation-triggers\"")
            .await?;
        seed_run_commit_outcome_fixtures(app).await?;
        assert_run_log_commit_fixture(
            driver,
            app,
            501,
            "status-completed",
            "Done. Created browser-test commit fixture.",
            "committed 0123456789ab (required)",
        )
        .await?;
        assert_run_output_fixture(driver, app).await?;
        assert_run_log_commit_fixture(
            driver,
            app,
            502,
            "status-failed",
            "Missing required commit: completed run left uncommitted changes.",
            "missing required commit (required)",
        )
        .await?;

        Ok(())
    }
}

async fn assert_run_output_fixture(
    driver: &WebDriver,
    app: &DispatchTestApp,
) -> Result<(), Report> {
    driver
        .goto(app.url("/projects/demo/automation/runs/503/log"))
        .await
        .context("failed to open compact run-output fixture")?;
    let toggle = find(driver, By::Css(".thinking-history-toggle")).await?;
    assert_that!(
        toggle
            .text()
            .await
            .context("failed to read thinking toggle")?
    )
    .is_equal_to("Thinking (1)");
    assert_that!(
        driver
            .find_all(By::Css(".output-reasoning-history"))
            .await
            .context("failed to inspect hidden thinking history")?
            .len()
    )
    .is_equal_to(0);

    let exploring = find(driver, By::Css(".command-exploring")).await?;
    assert_that!(
        exploring
            .text()
            .await
            .context("failed to read exploring summary")?
    )
    .is_equal_to("Exploring design/ui.md...");
    find(
        driver,
        By::XPath("//code[normalize-space()='Ran just check']"),
    )
    .await?;
    assert_source_does_not_contain(driver, "exit 0").await?;
    assert_source_does_not_contain(driver, "Command details").await?;
    assert_source_does_not_contain(driver, "Hide full output").await?;

    let preview = find(driver, By::Css(".tool-output-preview")).await?;
    assert_that!(
        preview
            .text()
            .await
            .context("failed to read compact output preview")?
    )
    .is_equal_to("line one\nline two");
    click(driver, By::Css(".tool-output-block summary")).await?;
    assert_that!(
        find(
            driver,
            By::Css(".tool-output-block[open] .tool-output-full")
        )
        .await?
        .text()
        .await
        .context("failed to read expanded command output")?
    )
    .is_equal_to("line one\nline two\nline three\nline four");
    click(
        driver,
        By::Css(".tool-output-block[open] .tool-output-full"),
    )
    .await?;
    assert_that!(
        find(driver, By::Css(".tool-output-block"))
            .await?
            .attr("open")
            .await
            .context("failed to inspect collapsed command output")?
            .is_none()
    )
    .is_true();

    find(
        driver,
        By::XPath("//code[normalize-space()='Ran git diff -- design/ui.md']"),
    )
    .await?;
    let diff_row = find(
        driver,
        By::XPath(
            "//article[contains(@class, 'output-command')][.//code[normalize-space()='Ran git diff -- design/ui.md']]",
        ),
    )
    .await?;
    let details = diff_row
        .find(By::Css(".tool-output-block"))
        .await
        .context("failed to find collapsed diff disclosure")?;
    let preview = details
        .find(By::Css(".tool-output-preview"))
        .await
        .context("failed to find collapsed diff preview")?;
    let full = details
        .find(By::Css(".tool-output-full"))
        .await
        .context("failed to find collapsed diff full output")?;
    let collapsed_diff_visibility = format!(
        "{}|{}|{}",
        details
            .attr("open")
            .await
            .context("failed to inspect collapsed diff disclosure state")?
            .is_some(),
        preview
            .css_value("display")
            .await
            .context("failed to inspect collapsed diff preview visibility")?,
        full.css_value("display")
            .await
            .context("failed to inspect collapsed diff full-output visibility")?,
    );
    assert_that!(collapsed_diff_visibility).is_equal_to("false|block|none");
    click(
        driver,
        By::XPath(
            "//article[.//code[normalize-space()='Ran git diff -- design/ui.md']]//details[contains(@class, 'tool-output-block')]/summary",
        ),
    )
    .await?;
    let expanded_diff_visibility = format!(
        "{}|{}|{}",
        details
            .attr("open")
            .await
            .context("failed to inspect expanded diff disclosure state")?
            .is_some(),
        preview
            .css_value("display")
            .await
            .context("failed to inspect expanded diff preview visibility")?,
        full.css_value("display")
            .await
            .context("failed to inspect expanded diff full-output visibility")?,
    );
    assert_that!(expanded_diff_visibility).is_equal_to("true|none|block");

    click(driver, By::Css(".thinking-history-toggle leptonic-toggle")).await?;
    let history = find(driver, By::Css(".output-reasoning-history")).await?;
    assert_that!(
        history
            .text()
            .await
            .context("failed to read thinking history row")?
    )
    .is_equal_to("Thought for 8s");
    assert_that!(
        driver
            .find_all(By::Css(".output-reasoning-history details"))
            .await
            .context("failed to inspect empty thinking disclosures")?
            .len()
    )
    .is_equal_to(0);

    let output = find(driver, By::Css(".model-output")).await?;
    let section = output
        .find(By::XPath("ancestor::section[1]"))
        .await
        .context("failed to find model-output section")?;
    let tool_output = find(driver, By::Css(".tool-output-preview")).await?;
    let colors = format!(
        "{}|{}|{}",
        output
            .css_value("color")
            .await
            .context("failed to inspect model-output color")?,
        section
            .css_value("background-color")
            .await
            .context("failed to inspect run-output section background")?,
        tool_output
            .css_value("color")
            .await
            .context("failed to inspect tool-output color")?,
    );
    assert_that!(colors)
        .is_equal_to("rgba(32, 36, 42, 1)|rgba(255, 255, 255, 1)|rgba(104, 116, 130, 1)");

    Ok(())
}

async fn assert_run_log_commit_fixture(
    driver: &WebDriver,
    app: &DispatchTestApp,
    run_id: i64,
    expected_result_class: &str,
    expected_summary: &str,
    expected_commit: &str,
) -> Result<(), Report> {
    driver
        .goto(app.url(&format!("/projects/demo/automation/runs/{run_id}/log")))
        .await
        .context_with(|| format!("failed to open run #{run_id} log page"))?;
    find(
        driver,
        By::XPath(format!(
            "//main[contains(@class, 'run-log')]//h1[normalize-space()='Run #{run_id}']"
        )),
    )
    .await?;
    find(
        driver,
        By::XPath(
            "//main[contains(@class, 'run-log')]//h2[normalize-space()='Developer instructions']",
        ),
    )
    .await?;
    find(
        driver,
        By::XPath("//main[contains(@class, 'run-log')]//h2[normalize-space()='User prompt']"),
    )
    .await?;

    let summary = run_log_detail_text(driver, "result").await?;
    assert_that!(summary).is_equal_to(expected_summary.to_owned());
    let result_class = find(driver, By::Css("main.run-log .run-result-inline"))
        .await?
        .class_name()
        .await
        .context("failed to inspect run result class")?
        .unwrap_or_default();
    assert_that!(result_class).contains(expected_result_class);

    let commit = run_log_detail_text(driver, "commit").await?;
    assert_that!(commit).is_equal_to(expected_commit.to_owned());
    Ok(())
}

async fn run_log_detail_text(driver: &WebDriver, term: &str) -> Result<String, Report> {
    let value = find(
        driver,
        By::XPath(format!(
            "//main[contains(@class, 'run-log')]//dt[normalize-space()={term:?}]/following-sibling::dd[1]"
        )),
    )
    .await?
    .text()
    .await
    .context_with(|| format!("failed to read run-log detail {term:?}"))?;
    if value.is_empty() {
        bail!("missing run-log detail {term:?}");
    }
    Ok(value)
}
