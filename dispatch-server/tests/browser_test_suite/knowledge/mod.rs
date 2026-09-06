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
        .is_equal_to(zoomed);
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
