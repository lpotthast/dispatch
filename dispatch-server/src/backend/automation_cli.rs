use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

use rootcause::{Result, prelude::*};
use tokio::process::Command;

const DEVELOPMENT_ENV: &str = "DISPATCH_DEVELOPMENT";
const CLI_HELP_MARKER: &str = "Dispatch agent-facing API relay";

pub(crate) async fn dispatch_cli_path() -> Result<PathBuf> {
    if development_mode()? {
        let dev_cli = development_cli_sources()?;
        return build_dev_dispatch_cli(
            &dev_cli,
            dispatch_cli_target_dir(),
            env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")),
        )
        .await;
    }

    let path = installed_dispatch_cli_path()?;
    verify_published_dispatch_cli(&path).await?;
    Ok(path)
}

fn development_mode() -> Result<bool> {
    parse_development_mode(env::var_os(DEVELOPMENT_ENV).as_deref())
}

fn parse_development_mode(value: Option<&OsStr>) -> Result<bool> {
    let Some(value) = value else {
        return Ok(false);
    };
    let value = value
        .to_str()
        .ok_or_else(|| report!("{DEVELOPMENT_ENV} must be valid UTF-8"))?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Ok(true),
        "" | "0" | "false" => Ok(false),
        _ => bail!("{DEVELOPMENT_ENV} must be one of: 1, true, 0, false"),
    }
}

fn development_cli_sources() -> Result<DevDispatchCli> {
    let server_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = server_manifest_dir.parent().ok_or_else(|| {
        report!(
            "failed to resolve Dispatch repository root from {}",
            server_manifest_dir.display()
        )
    })?;
    let manifest = repo_root.join("dispatch-cli/Cargo.toml");
    if !manifest.is_file() {
        bail!(
            "{DEVELOPMENT_ENV}=1 requires Dispatch sources, but the CLI manifest was not found at {}",
            manifest.display()
        );
    }
    Ok(DevDispatchCli {
        repo_root: repo_root.to_path_buf(),
        manifest,
    })
}

#[derive(Debug)]
struct DevDispatchCli {
    repo_root: PathBuf,
    manifest: PathBuf,
}

fn dispatch_cli_target_dir() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .filter(|value| !value.is_empty())
        .or_else(|| env::var_os("DISPATCH_CLI_TARGET_DIR").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("dispatch-cli-target"))
}

async fn build_dev_dispatch_cli(
    dev_cli: &DevDispatchCli,
    target_dir: PathBuf,
    cargo: OsString,
) -> Result<PathBuf> {
    let output = Command::new(&cargo)
        .arg("build")
        .arg("-q")
        .arg("--manifest-path")
        .arg(&dev_cli.manifest)
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(&dev_cli.repo_root)
        .output()
        .await
        .context_with(|| {
            format!(
                "failed to build Dispatch agent-facing CLI with {}",
                Path::new(&cargo).display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to build Dispatch agent-facing CLI from {} (status {}): {}",
            dev_cli.manifest.display(),
            output.status,
            stderr.trim()
        );
    }

    let absolute_target_dir = if target_dir.is_absolute() {
        target_dir
    } else {
        dev_cli.repo_root.join(target_dir)
    };
    ensure_dispatch_cli_path(
        absolute_target_dir
            .join("debug")
            .join(format!("dispatch{}", env::consts::EXE_SUFFIX)),
    )
}

fn installed_dispatch_cli_path() -> Result<PathBuf> {
    let path = env::var_os("PATH")
        .ok_or_else(|| report!("PATH is not set; the published Dispatch CLI cannot be located"))?;
    find_dispatch_cli_in_path(&path).ok_or_else(|| {
        report!(
            "the published Dispatch agent-facing CLI '{}' is not executable on PATH; install it alongside dispatch-server or run the source checkout with {DEVELOPMENT_ENV}=1",
            dispatch_executable_name()
        )
    })
}

async fn verify_published_dispatch_cli(path: &Path) -> Result<()> {
    let output = Command::new(path)
        .arg("--help")
        .output()
        .await
        .context_with(|| {
            format!(
                "failed to execute published Dispatch CLI {}",
                path.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "published Dispatch CLI {} failed its availability check (status {}): {}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(CLI_HELP_MARKER) {
        bail!(
            "executable '{}' on PATH is not the Dispatch agent-facing CLI",
            path.display()
        );
    }
    Ok(())
}

fn find_dispatch_cli_in_path(path: &OsStr) -> Option<PathBuf> {
    env::split_paths(path).find_map(|directory| {
        let candidate = directory.join(dispatch_executable_name());
        is_executable_file(&candidate).then(|| candidate.canonicalize().unwrap_or(candidate))
    })
}

fn dispatch_executable_name() -> String {
    format!("dispatch{}", env::consts::EXE_SUFFIX)
}

fn ensure_dispatch_cli_path(path: PathBuf) -> Result<PathBuf> {
    if !is_executable_file(&path) {
        bail!(
            "Dispatch agent-facing CLI path '{}' does not exist or is not executable",
            path.display()
        );
    }
    Ok(path)
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn development_mode_requires_an_explicit_boolean_value() {
        assert!(!parse_development_mode(None).unwrap());
        assert!(!parse_development_mode(Some(OsStr::new("0"))).unwrap());
        assert!(!parse_development_mode(Some(OsStr::new("false"))).unwrap());
        assert!(parse_development_mode(Some(OsStr::new("1"))).unwrap());
        assert!(parse_development_mode(Some(OsStr::new("TRUE"))).unwrap());
        assert!(parse_development_mode(Some(OsStr::new("yes"))).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn published_cli_lookup_uses_path_order_and_requires_an_executable() {
        let temp = TempDir::new().unwrap();
        let first_bin = temp.path().join("first");
        let second_bin = temp.path().join("second");
        fs::create_dir(&first_bin).unwrap();
        fs::create_dir(&second_bin).unwrap();
        let first_cli = first_bin.join(dispatch_executable_name());
        let second_cli = second_bin.join(dispatch_executable_name());
        fs::write(&first_cli, "not executable").unwrap();
        fs::write(&second_cli, "#!/bin/sh\n").unwrap();
        let mut permissions = fs::metadata(&second_cli).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&second_cli, permissions).unwrap();
        let path = env::join_paths([&first_bin, &second_bin]).unwrap();
        let expected = second_cli.canonicalize().unwrap();

        assert_eq!(
            find_dispatch_cli_in_path(&path).as_deref(),
            Some(expected.as_path())
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn published_cli_availability_check_rejects_the_wrong_dispatch_binary() {
        let temp = TempDir::new().unwrap();
        let cli = temp.path().join("dispatch");
        fs::write(&cli, "#!/bin/sh\nprintf 'unrelated dispatch command\\n'\n").unwrap();
        let mut permissions = fs::metadata(&cli).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&cli, permissions).unwrap();

        let err = verify_published_dispatch_cli(&cli).await.unwrap_err();

        assert!(
            err.to_string()
                .contains("is not the Dispatch agent-facing CLI")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn published_cli_availability_check_executes_help() {
        let temp = TempDir::new().unwrap();
        let cli = temp.path().join("dispatch");
        fs::write(
            &cli,
            "#!/bin/sh\nprintf 'Dispatch agent-facing API relay\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&cli).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&cli, permissions).unwrap();

        verify_published_dispatch_cli(&cli).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn builds_development_cli_before_returning_agent_executable() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo with spaces");
        let manifest = repo_root.join("dispatch-cli/Cargo.toml");
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(&manifest, "[package]\nname = \"dispatch-cli\"\n").unwrap();

        let fake_cargo = temp.path().join("fake cargo");
        fs::write(
            &fake_cargo,
            concat!(
                "#!/bin/sh\n",
                "set -eu\n",
                "mkdir -p \"$CARGO_TARGET_DIR/debug\"\n",
                ": > \"$CARGO_TARGET_DIR/debug/dispatch\"\n",
                "chmod +x \"$CARGO_TARGET_DIR/debug/dispatch\"\n",
                "printf '%s\\n' \"$@\" > \"$CARGO_TARGET_DIR/args\"\n",
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_cargo).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_cargo, permissions).unwrap();
        let target_dir = temp.path().join("target with spaces");

        let binary = build_dev_dispatch_cli(
            &DevDispatchCli {
                repo_root,
                manifest: manifest.clone(),
            },
            target_dir.clone(),
            fake_cargo.into_os_string(),
        )
        .await
        .unwrap();

        assert_eq!(binary, target_dir.join("debug/dispatch"));
        assert!(binary.is_file());
        let args = fs::read_to_string(target_dir.join("args")).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            ["build", "-q", "--manifest-path", manifest.to_str().unwrap()]
        );
    }
}
