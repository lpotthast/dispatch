#![cfg(not(target_arch = "wasm32"))]

mod browser_test_suite;

use std::time::Duration;

use browser_test::thirtyfour::ChromiumLikeCapabilities;
use browser_test::{
    BrowserTestFailurePolicy, BrowserTestParallelism, BrowserTestRunner, BrowserTestVisibility,
    BrowserTimeouts, CancellationToken, ChromeBinary, PauseConfig,
};
use browser_test_suite::{DispatchTestApp, DispatchTestAppStartError, tests, write_signal_probe};
use leptos_browser_test::Report;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn browser_tests() -> Result<(), Report> {
    #[cfg(unix)]
    {
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        return run_with_unix_signals(&mut sigint, &mut sigterm).await;
    }

    #[cfg(not(unix))]
    {
        run_with_ctrl_c().await
    }
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum ExternalSignal {
    Sigint,
    Sigterm,
}

#[cfg(unix)]
impl ExternalSignal {
    const fn exit_code(self) -> i32 {
        match self {
            Self::Sigint => 130,
            Self::Sigterm => 143,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Sigint => "SIGINT",
            Self::Sigterm => "SIGTERM",
        }
    }
}

#[cfg(unix)]
async fn run_with_unix_signals(
    sigint: &mut tokio::signal::unix::Signal,
    sigterm: &mut tokio::signal::unix::Signal,
) -> Result<(), Report> {
    tracing_subscriber::fmt().try_init().ok();
    let cancellation = CancellationToken::new();
    let suite = run_suite(&cancellation);
    tokio::pin!(suite);
    let mut original_signal = None;

    let result = loop {
        tokio::select! {
            biased;
            Some(()) = sigint.recv() => {
                record_signal(ExternalSignal::Sigint, &mut original_signal, &cancellation);
            }
            Some(()) = sigterm.recv() => {
                record_signal(ExternalSignal::Sigterm, &mut original_signal, &cancellation);
            }
            result = &mut suite => {
                if original_signal.is_some() {
                    drain_late_signals(sigint, sigterm, &mut original_signal, &cancellation).await;
                }
                break result;
            }
        }
    };

    if let Some(signal) = original_signal {
        if let Err(err) = result {
            eprintln!(
                "browser-test cleanup after {} reported:\n{err:?}",
                signal.label()
            );
        }
        std::process::exit(signal.exit_code());
    }

    result
}

#[cfg(unix)]
async fn drain_late_signals(
    sigint: &mut tokio::signal::unix::Signal,
    sigterm: &mut tokio::signal::unix::Signal,
    original_signal: &mut Option<ExternalSignal>,
    cancellation: &CancellationToken,
) {
    let observation_window = tokio::time::sleep(Duration::from_millis(250));
    tokio::pin!(observation_window);
    loop {
        tokio::select! {
            biased;
            Some(()) = sigint.recv() => {
                record_signal(ExternalSignal::Sigint, original_signal, cancellation);
            }
            Some(()) = sigterm.recv() => {
                record_signal(ExternalSignal::Sigterm, original_signal, cancellation);
            }
            () = &mut observation_window => break,
        }
    }
}

#[cfg(unix)]
fn record_signal(
    signal: ExternalSignal,
    original_signal: &mut Option<ExternalSignal>,
    cancellation: &CancellationToken,
) {
    let first = original_signal.is_none();
    if let Err(err) = write_signal_probe(
        "signal_received",
        json!({ "signal": signal.label(), "first": first }),
    ) {
        tracing::error!(error = ?err, "failed to record browser-test signal probe");
    }
    if first {
        *original_signal = Some(signal);
        tracing::warn!(signal = signal.label(), "external shutdown requested");
        cancellation.cancel();
    } else {
        tracing::warn!(
            signal = signal.label(),
            "additional shutdown signal received during cleanup"
        );
    }
}

#[cfg(not(unix))]
async fn run_with_ctrl_c() -> Result<(), Report> {
    tracing_subscriber::fmt().try_init().ok();
    let cancellation = CancellationToken::new();
    let suite = run_suite(&cancellation);
    tokio::pin!(suite);

    tokio::select! {
        result = &mut suite => result,
        result = tokio::signal::ctrl_c() => {
            result?;
            cancellation.cancel();
            let result = suite.await;
            if let Err(err) = result {
                eprintln!("browser-test cleanup after Ctrl-C reported:\n{err:?}");
            }
            std::process::exit(130);
        }
    }
}

async fn run_suite(cancellation: &CancellationToken) -> Result<(), Report> {
    let app = match DispatchTestApp::start(cancellation.clone()).await {
        Ok(app) => app,
        Err(DispatchTestAppStartError::StartupCancelled(err)) => {
            tracing::info!(cancellation = ?err, "Dispatch browser-test startup cancelled after cleanup");
            return Ok(());
        }
        Err(DispatchTestAppStartError::Failed(err)) => return Err(err),
    };

    let browser_visibility = BrowserTestVisibility::from_env();
    let run_chrome_single_process = browser_visibility.resolve().is_headless();
    let webdriver_port = configured_port("DISPATCH_BROWSER_TEST_WEBDRIVER_PORT")?;
    let devtools_port = configured_port("DISPATCH_BROWSER_TEST_DEVTOOLS_PORT")?;

    let runner = BrowserTestRunner::new()
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
            if let Some(devtools_port) = devtools_port {
                caps.add_arg(&format!("--remote-debugging-port={devtools_port}"))?;
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
        );
    let runner = match webdriver_port {
        Some(port) => runner.with_webdriver_port(port),
        None => runner,
    };
    write_signal_probe(
        "browser_run_starting",
        json!({ "webdriver_port": webdriver_port, "devtools_port": devtools_port }),
    )?;
    let browser_result = runner
        .run(cancellation.clone(), &app, tests())
        .await
        .map_err(Report::into_dynamic);

    let app_shutdown_result = app.shutdown().await;
    match (browser_result, app_shutdown_result) {
        (Ok(_outcome), Ok(())) => Ok(()),
        (Err(err), Ok(())) => Err(err),
        (Ok(_outcome), Err(err)) => Err(err),
        (Err(browser_err), Err(mut shutdown_err)) => {
            shutdown_err
                .children_mut()
                .push(browser_err.into_cloneable());
            Err(shutdown_err)
        }
    }
}

fn configured_port(name: &str) -> Result<Option<u16>, Report> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };
    let value = value.into_string().map_err(|_| {
        Report::new_sendsync(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must contain Unicode digits"),
        ))
    })?;
    Ok(Some(value.parse().map_err(|err| {
        Report::new_sendsync(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must be a valid TCP port: {err}"),
        ))
    })?))
}
