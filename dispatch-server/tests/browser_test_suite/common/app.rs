use std::{
    env, fs,
    path::{Path, PathBuf},
};

use browser_test::CancellationToken;
use leptos_browser_test::{
    LeptosBrowserTestError, LeptosTestApp, LeptosTestAppConfig, Report, ResultExt,
};
use serde_json::{Value, json};
use tempfile::TempDir;

pub(crate) struct DispatchTestApp {
    app: LeptosTestApp,
    tmpdir: TempDir,
    pub(crate) database: PathBuf,
    base_url: String,
}

pub(crate) enum DispatchTestAppStartError {
    StartupCancelled(leptos_browser_test::Report<LeptosBrowserTestError>),
    Failed(Report),
}

impl DispatchTestApp {
    pub(crate) async fn start(
        cancellation: CancellationToken,
    ) -> Result<Self, DispatchTestAppStartError> {
        let tmpdir = tempfile::tempdir()
            .context("failed to create Dispatch browser-test temp dir")
            .map_err(|err| DispatchTestAppStartError::Failed(err.into_dynamic()))?;
        if let Err(err) = write_signal_probe(
            "app_starting",
            json!({
                "temp_dir": tmpdir.path().display().to_string(),
                "pid": std::process::id(),
            }),
        ) {
            return Err(close_tempdir_after_failure(tmpdir, err));
        }

        let (database, test_path) = match prepare_test_environment(&tmpdir) {
            Ok(prepared) => prepared,
            Err(err) => return Err(close_tempdir_after_failure(tmpdir, err)),
        };
        let app_config = LeptosTestAppConfig::new(env!("CARGO_MANIFEST_DIR"))
            .with_app_name("dispatch browser test")
            .with_forward_logs(true)
            .with_startup_line("Serving Dispatch")
            .with_env("DISPATCH_DATABASE", database.as_os_str())
            .with_env("HOME", tmpdir.path().as_os_str())
            .with_env("PATH", test_path.as_os_str());
        let app_config = match apply_configured_ports(app_config) {
            Ok(config) => config,
            Err(err) => return Err(close_tempdir_after_failure(tmpdir, err)),
        };

        let app = match app_config.start(cancellation).await {
            Ok(app) => app,
            Err(err)
                if matches!(
                    err.current_context(),
                    LeptosBrowserTestError::StartupCancelled { .. }
                ) =>
            {
                return match tmpdir.close() {
                    Ok(()) => Err(DispatchTestAppStartError::StartupCancelled(err)),
                    Err(close_err) => Err(DispatchTestAppStartError::Failed(attach_child(
                        Report::new_sendsync(close_err)
                            .context("failed to remove Dispatch browser-test temp dir")
                            .into_dynamic(),
                        err.into_dynamic(),
                    ))),
                };
            }
            Err(err) => return Err(close_tempdir_after_failure(tmpdir, err.into_dynamic())),
        };

        let base_url = app.base_url().to_owned();
        if let Err(err) = write_signal_probe(
            "app_ready",
            json!({
                "base_url": base_url,
                "reload_port": app.reload_port(),
                "cargo_leptos_pid": app.process_id(),
            }),
        ) {
            let cleanup_result = app
                .shutdown()
                .await
                .map(|_| ())
                .map_err(Report::into_dynamic);
            return Err(DispatchTestAppStartError::Failed(close_after_app_cleanup(
                tmpdir,
                cleanup_result,
                err,
            )));
        }

        Ok(Self {
            app,
            tmpdir,
            database,
            base_url,
        })
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub(crate) fn temp_dir(&self) -> &Path {
        self.tmpdir.path()
    }

    pub(crate) async fn shutdown(self) -> Result<(), Report> {
        let Self { app, tmpdir, .. } = self;
        let app_result = app
            .shutdown()
            .await
            .map(|_| ())
            .map_err(Report::into_dynamic);
        let temp_result = tmpdir
            .close()
            .context("failed to remove Dispatch browser-test temp dir")
            .map_err(Report::into_dynamic);
        merge_cleanup_results(app_result, temp_result)
    }
}

pub(crate) fn write_signal_probe(event: &str, details: Value) -> Result<(), Report> {
    let Some(path) = env::var_os("DISPATCH_BROWSER_TEST_METADATA") else {
        return Ok(());
    };
    use std::io::Write as _;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .context("failed to open browser-test signal metadata")?;
    serde_json::to_writer(&mut file, &json!({ "event": event, "details": details }))
        .context("failed to write browser-test signal metadata")?;
    file.write_all(b"\n")
        .context("failed to terminate browser-test signal metadata record")?;
    file.sync_all()
        .context("failed to flush browser-test signal metadata")?;
    Ok(())
}

fn prepare_test_environment(tmpdir: &TempDir) -> Result<(PathBuf, std::ffi::OsString), Report> {
    let database = tmpdir.path().join("dispatch.sqlite3");
    let editor_bin_dir = tmpdir.path().join("bin");
    fs::create_dir(&editor_bin_dir).context("failed to create browser-test editor bin dir")?;
    for program in [
        "rustrover",
        "rustrover64.exe",
        "code",
        "code.cmd",
        "code.exe",
    ] {
        write_test_executable(&editor_bin_dir.join(program))?;
    }
    Ok((database, path_with_prefix(&editor_bin_dir)?))
}

fn apply_configured_ports(mut config: LeptosTestAppConfig) -> Result<LeptosTestAppConfig, Report> {
    if let Some(app_port) = configured_port("DISPATCH_BROWSER_TEST_APP_PORT")? {
        config = config.with_site_addr(format!("127.0.0.1:{app_port}"));
    }
    if let Some(reload_port) = configured_port("DISPATCH_BROWSER_TEST_RELOAD_PORT")? {
        config = config.with_reload_port(reload_port);
    }
    Ok(config)
}

fn configured_port(name: &str) -> Result<Option<u16>, Report> {
    let Some(value) = env::var_os(name) else {
        return Ok(None);
    };
    let value = value.into_string().map_err(|_| {
        Report::new_sendsync(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name} must contain Unicode digits"),
        ))
    })?;
    Ok(Some(
        value
            .parse()
            .context(format!("{name} must be a valid TCP port"))
            .map_err(Report::into_dynamic)?,
    ))
}

fn close_tempdir_after_failure(
    tmpdir: TempDir,
    operation_err: Report,
) -> DispatchTestAppStartError {
    match tmpdir.close() {
        Ok(()) => DispatchTestAppStartError::Failed(operation_err),
        Err(err) => DispatchTestAppStartError::Failed(attach_child(
            Report::new_sendsync(err)
                .context("failed to remove Dispatch browser-test temp dir")
                .into_dynamic(),
            operation_err,
        )),
    }
}

fn close_after_app_cleanup(
    tmpdir: TempDir,
    app_result: Result<(), Report>,
    operation_err: Report,
) -> Report {
    let mut primary = match app_result {
        Ok(()) => operation_err,
        Err(cleanup_err) => attach_child(cleanup_err, operation_err),
    };
    if let Err(err) = tmpdir.close() {
        primary = attach_child(
            primary,
            Report::new_sendsync(err)
                .context("failed to remove Dispatch browser-test temp dir")
                .into_dynamic(),
        );
    }
    primary
}

fn merge_cleanup_results(
    app_result: Result<(), Report>,
    temp_result: Result<(), Report>,
) -> Result<(), Report> {
    match (app_result, temp_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), Ok(())) | (Ok(()), Err(err)) => Err(err),
        (Err(app_err), Err(temp_err)) => Err(attach_child(app_err, temp_err)),
    }
}

fn attach_child(mut primary: Report, child: Report) -> Report {
    primary.children_mut().push(child.into_cloneable());
    primary
}

fn path_with_prefix(prefix: &Path) -> Result<std::ffi::OsString, Report> {
    let mut entries = vec![prefix.to_path_buf()];
    if let Some(path) = env::var_os("PATH") {
        entries.extend(env::split_paths(&path));
    }
    Ok(env::join_paths(entries).context("failed to build browser-test PATH")?)
}

fn write_test_executable(path: &Path) -> Result<(), Report> {
    fs::write(path, "#!/bin/sh\nexit 0\n").context("failed to write browser-test editor shim")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .context("failed to stat browser-test editor shim")?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .context("failed to make browser-test editor shim executable")?;
    }
    Ok(())
}
