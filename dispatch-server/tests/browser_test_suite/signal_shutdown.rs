use std::borrow::Cow;

use browser_test::thirtyfour::WebDriver;
use browser_test::{BrowserTest, async_trait};
use leptos_browser_test::Report;
use serde_json::json;

use super::common::{DispatchTestApp, write_signal_probe};

pub(crate) struct SignalShutdownTest;

#[async_trait]
impl BrowserTest<DispatchTestApp> for SignalShutdownTest {
    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("external signal shutdown")
    }

    async fn run(&self, _driver: &WebDriver, _app: &DispatchTestApp) -> Result<(), Report> {
        write_signal_probe("browser_active", json!({ "pid": std::process::id() }))?;
        std::future::pending().await
    }
}
