use super::common::*;
use assertr::prelude::*;
use browser_test::{
    BrowserTest, async_trait,
    thirtyfour::{By, WebDriver},
};
use leptos_browser_test::{Report, ResultExt};
use std::{borrow::Cow, fs};

pub(crate) struct KnowledgeReaderTest;
#[async_trait]
impl BrowserTest<DispatchTestApp> for KnowledgeReaderTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("knowledge graph, editor drawer, and exclusion refresh")
    }
    async fn run(&self, driver: &WebDriver, app: &DispatchTestApp) -> Result<(), Report> {
        let workspace = app.temp_dir().join("knowledge-reader");
        fs::create_dir_all(workspace.join("knowledge"))?;
        fs::write(
            workspace.join("knowledge/README.md"),
            "---\nid: project\n---\n# Reader fixture\n\nA small project overview.\n",
        )?;
        fs::write(
            workspace.join("knowledge/detail.md"),
            "---\nid: detail\nrefines: [project]\ncustom: preserved\n---\n# Detail fixture\n\nInitial behavior.\n",
        )?;
        driver.goto(app.url("/projects")).await?;
        let response = browser_request(driver, reqwest::Method::POST, "/projects")
            .await?
            .form(&[
                ("name", "knowledge-reader"),
                ("display_name", "Knowledge reader"),
                ("path", workspace.to_str().unwrap()),
            ])
            .send()
            .await?;
        response_text(response, "create isolated knowledge fixture").await?;
        driver
            .goto(app.url("/knowledge?project=knowledge-reader"))
            .await?;
        click(driver, By::Css("[data-knowledge-path='detail.md']")).await?;
        wait_until("selected graph document opens", || async {
            Ok(find(driver, By::Css(".knowledge-document-content"))
                .await?
                .text()
                .await?
                .contains("Initial behavior")
                .then_some(()))
        })
        .await?;
        let before = find(driver, By::Css(".knowledge-graph-world"))
            .await?
            .attr("transform")
            .await?;
        click(driver, By::Css("button[aria-label='Zoom in']")).await?;
        let zoomed = find(driver, By::Css(".knowledge-graph-world"))
            .await?
            .attr("transform")
            .await?;
        assert_that!(&(before != zoomed)).is_true();
        click(driver, By::Css("[data-knowledge-tool='Edit document']")).await?;
        let input = find(driver, By::Css("textarea[aria-label='Document Markdown']")).await?;
        let edited = input
            .value()
            .await?
            .unwrap()
            .replace("Initial behavior", "Edited through Dispatch");
        input.clear().await?;
        input.send_keys(&edited).await?;
        click(driver, By::XPath("//button[normalize-space()='Save']")).await?;
        wait_until("Markdown save reaches disk", || async {
            Ok(fs::read_to_string(workspace.join("knowledge/detail.md"))?
                .contains("Edited through Dispatch")
                .then_some(()))
        })
        .await?;
        assert_that!(&fs::read_to_string(workspace.join("knowledge/detail.md"))?)
            .contains("custom: preserved");
        wait_until("save completes in the drawer", || async {
            Ok(find(driver, By::Css(".knowledge-message"))
                .await?
                .text()
                .await?
                .contains("Document saved")
                .then_some(()))
        })
        .await?;
        click(
            driver,
            By::Css(".knowledge-tool-drawer.open .knowledge-tool-drawer-heading button"),
        )
        .await?;
        fs::write(
            workspace.join("knowledge/detail.md"),
            edited.replace("Edited through Dispatch", "Edited in a regular editor"),
        )?;
        click(driver, By::Css("button[aria-label='Refresh knowledge']")).await?;
        wait_until("external edit refresh preserves selection", || async {
            Ok(find(driver, By::Css(".knowledge-document-content"))
                .await?
                .text()
                .await?
                .contains("Edited in a regular editor")
                .then_some(()))
        })
        .await?;
        assert_that!(
            &find(driver, By::Css(".knowledge-graph-world"))
                .await?
                .attr("transform")
                .await?
        )
        .is_equal_to(zoomed.clone());
        // Keep the deterministic UI fixture queued without allocating a provider process.
        use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
        let db = Database::connect(format!("sqlite://{}?mode=rwc", app.database.display())).await?;
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE projects SET max_read_only_agents=0 WHERE name='knowledge-reader'".to_owned(),
        ))
        .await?;
        click(driver, By::Css("[data-knowledge-tool='Knowledge jobs']")).await?;
        assert_that!(
            &find(driver, By::Css(".knowledge-jobs"))
                .await?
                .text()
                .await?
        )
        .contains("Recurring automation is disabled");
        click(driver,By::XPath("//div[contains(@class,'knowledge-jobs')]//button[normalize-space()='Continue discovery']")).await?;
        wait_until("knowledge job admission is visible", || async {
            Ok(find(driver, By::Css(".knowledge-job-detail"))
                .await?
                .text()
                .await?
                .contains("Job #")
                .then_some(()))
        })
        .await?;
        wait_until("queued job has captured its inventory", || async {
            Ok(find(driver, By::Css(".knowledge-activity"))
                .await?
                .text()
                .await?
                .contains("Discovery")
                .then_some(()))
        })
        .await?;
        let aspect = find(driver, By::Css("input[placeholder='All aspects']")).await?;
        aspect.send_keys("recovery").await?;
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        assert_that!(&driver.active_element().await?.attr("placeholder").await?)
            .is_equal_to(Some("All aspects".to_owned()));
        assert_that!(&aspect.value().await?).is_equal_to(Some("recovery".to_owned()));
        let response = browser_request(
            driver,
            reqwest::Method::GET,
            "/api/projects/knowledge-reader/knowledge/jobs",
        )
        .await?
        .send()
        .await?;
        let jobs: Vec<serde_json::Value> = response.json().await?;
        assert_that!(&jobs.len()).is_equal_to(1);
        let original_id = jobs[0]["id"].as_i64().unwrap();
        let retry = browser_request(
            driver,
            reqwest::Method::POST,
            "/api/projects/knowledge-reader/knowledge/jobs",
        )
        .await?
        .json(&jobs[0]["request"])
        .send()
        .await?;
        let retry: serde_json::Value = retry.json().await?;
        assert_that!(&retry["id"].as_i64()).is_equal_to(Some(original_id));
        click(
            driver,
            By::XPath("//button[normalize-space()='Cancel job']"),
        )
        .await?;
        wait_until("cancellation is durable", || async {
            Ok(find(driver, By::Css(".knowledge-job-detail"))
                .await?
                .text()
                .await?
                .contains("Cancelled")
                .then_some(()))
        })
        .await?;
        click(
            driver,
            By::Css(".knowledge-tool-drawer.open .knowledge-tool-drawer-heading button"),
        )
        .await?;
        assert_that!(
            &find(driver, By::Css(".knowledge-document-content"))
                .await?
                .text()
                .await?
        )
        .contains("Edited in a regular editor");
        assert_that!(
            &find(driver, By::Css(".knowledge-graph-world"))
                .await?
                .attr("transform")
                .await?
        )
        .is_equal_to(zoomed.clone());
        fs::write(workspace.join(".dispatchignore"), "knowledge/detail.md\n")?;
        click(driver, By::Css("button[aria-label='Refresh knowledge']")).await?;
        wait_until("ignored document leaves graph and reader", || async {
            let nodes = driver
                .find_all(By::Css("[data-knowledge-path='detail.md']"))
                .await?;
            let body = find(driver, By::Css(".knowledge-document-content"))
                .await?
                .text()
                .await?;
            Ok((nodes.is_empty() && !body.contains("Edited in a regular editor")).then_some(()))
        })
        .await
        .context("knowledge workspace retained excluded content")?;
        assert_that!(&workspace.join("knowledge/detail.md").exists()).is_true();
        Ok(())
    }
}
