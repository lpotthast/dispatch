#![cfg(not(target_arch = "wasm32"))]

mod browser_test_suite;

use std::time::Duration;

use browser_test::thirtyfour::ChromiumLikeCapabilities;
use browser_test::{
    BrowserTestFailurePolicy, BrowserTestParallelism, BrowserTestRunner, BrowserTestVisibility,
    BrowserTimeouts, ChromeBinary, PauseConfig,
};
use browser_test_suite::{DispatchTestApp, tests};
use leptos_browser_test::Report;

#[tokio::test(flavor = "multi_thread")]
async fn browser_tests() -> Result<(), Report> {
    tracing_subscriber::fmt().init();

    let app = DispatchTestApp::start().await?;
    let browser_visibility = BrowserTestVisibility::from_env();
    let run_chrome_single_process = browser_visibility.resolve().is_headless();

    BrowserTestRunner::new()
        // Headless Shell avoids macOS app-registration calls that are unavailable in managed
        // test environments. Visible runs still use regular Chrome.
        .with_headless_chrome_binary(ChromeBinary::ChromeHeadlessShell)
        .with_chrome_capabilities(move |caps| {
            // Chrome's process sandbox can fail in nested/managed CI-style sandboxes. WebDriver
            // still runs in Dispatch's test process sandbox, so this only disables Chrome's own
            // child-process sandbox layer.
            caps.add_arg("--no-sandbox")?;
            if run_chrome_single_process {
                // The Codex SDK workspace sandbox on macOS denies Mach service registration. In
                // Headless Shell, Chromium otherwise registers
                // org.chromium.Chromium.MachPortRendezvousServer.<pid> before DevTools startup for
                // child-process rendezvous. Keeping the headless browser in one process avoids that
                // bootstrap_check_in path; visible debugging runs stay multi-process.
                caps.add_arg("--single-process")?;
            }
            // Avoid /dev/shm startup failures in restricted environments by using regular temp
            // files for Chrome IPC/shared-memory storage.
            caps.add_arg("--disable-dev-shm-usage")?;
            Ok(())
        })
        .with_test_parallelism(BrowserTestParallelism::Sequential)
        .with_failure_policy(BrowserTestFailurePolicy::RunAll)
        .with_visibility(browser_visibility)
        .with_pause(PauseConfig::from_env())
        .with_timeouts(
            BrowserTimeouts::builder()
                .implicit_wait_timeout(Duration::from_millis(100))
                .page_load_timeout(Duration::from_secs(20))
                .build(),
        )
        .run(&app, tests())
        .await
        .map_err(Report::into_dynamic)?;

    Ok(())
}
