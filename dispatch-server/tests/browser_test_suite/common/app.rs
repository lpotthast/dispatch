use std::{
    env, fs,
    path::{Path, PathBuf},
};

use leptos_browser_test::{LeptosTestApp, LeptosTestAppConfig, Report, ResultExt};
use tempfile::TempDir;

pub(crate) struct DispatchTestApp {
    _app: LeptosTestApp,
    _tmpdir: TempDir,
    pub(crate) database: PathBuf,
    base_url: String,
}

impl DispatchTestApp {
    pub(crate) async fn start() -> Result<Self, Report> {
        let tmpdir =
            tempfile::tempdir().context("failed to create Dispatch browser-test temp dir")?;
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
        let test_path = path_with_prefix(&editor_bin_dir)?;

        let app = LeptosTestAppConfig::new(env!("CARGO_MANIFEST_DIR"))
            .with_app_name("dispatch browser test")
            .with_forward_logs(true)
            .with_startup_line("Serving Dispatch")
            .with_env("DISPATCH_DATABASE", database.as_os_str())
            .with_env("HOME", tmpdir.path().as_os_str())
            .with_env("PATH", test_path.as_os_str())
            .start()
            .await
            .map_err(Report::into_dynamic)?;

        let base_url = app.base_url().to_owned();

        Ok(Self {
            _app: app,
            _tmpdir: tmpdir,
            database,
            base_url,
        })
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub(crate) fn temp_dir(&self) -> &Path {
        self._tmpdir.path()
    }
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
