use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
};

use dispatch_types::WorkspaceEditorView;
use rootcause::{Result, prelude::*};

use crate::backend::{projects, storage::Store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceOpenTarget {
    Folder,
    RustRover,
    VsCode,
}

impl WorkspaceOpenTarget {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "folder" => Ok(Self::Folder),
            "ide" | "rustrover" | "rust_rover" => Ok(Self::RustRover),
            "vscode" | "vs_code" | "code" => Ok(Self::VsCode),
            other => {
                bail!("workspace open target must be folder, rustrover, or vscode, got '{other}'")
            }
        }
    }
}

#[derive(Clone, Debug)]
struct WorkspaceOpenConfig {
    os: String,
    path_env: Option<OsString>,
    home: Option<OsString>,
}

impl WorkspaceOpenConfig {
    fn from_env() -> Self {
        Self {
            os: env::consts::OS.to_owned(),
            path_env: env::var_os("PATH"),
            home: env::var_os("HOME"),
        }
    }

    fn command_for(
        &self,
        target: WorkspaceOpenTarget,
        path: &Path,
    ) -> Result<WorkspaceOpenCommand> {
        match target {
            WorkspaceOpenTarget::Folder => folder_open_command(&self.os, path),
            WorkspaceOpenTarget::RustRover | WorkspaceOpenTarget::VsCode => {
                editor_open_command(self, target, path)
            }
        }
    }

    fn available_editors(&self) -> Vec<WorkspaceEditorView> {
        [
            (
                WorkspaceOpenTarget::RustRover,
                "RustRover",
                self.rustrover_available(),
            ),
            (
                WorkspaceOpenTarget::VsCode,
                "VS Code",
                self.vscode_available(),
            ),
        ]
        .into_iter()
        .filter(|(_, _, available)| *available)
        .map(|(target, label, _)| WorkspaceEditorView {
            target: workspace_editor_target_value(target).to_owned(),
            label: label.to_owned(),
        })
        .collect()
    }

    fn rustrover_available(&self) -> bool {
        match self.os.as_str() {
            "macos" => {
                macos_application_exists("RustRover", self.home.as_deref())
                    || program_in_path("rustrover", self.path_env.as_deref())
            }
            "windows" => program_in_path("rustrover64.exe", self.path_env.as_deref()),
            _ => program_in_path("rustrover", self.path_env.as_deref()),
        }
    }

    fn vscode_available(&self) -> bool {
        match self.os.as_str() {
            "macos" => {
                macos_application_exists("Visual Studio Code", self.home.as_deref())
                    || program_in_path("code", self.path_env.as_deref())
            }
            "windows" => {
                first_program_in_path(["code.cmd", "code.exe"], self.path_env.as_deref()).is_some()
            }
            _ => program_in_path("code", self.path_env.as_deref()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceOpenCommand {
    program: OsString,
    args: Vec<OsString>,
}

impl WorkspaceOpenCommand {
    async fn run(self) -> Result<()> {
        let output = tokio::process::Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .context_with(|| format!("failed to start {}", self.program.to_string_lossy()))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{} failed{}",
            self.program.to_string_lossy(),
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        )
    }
}

pub(crate) async fn project_workspace_path(store: &Store, project_name: &str) -> Result<PathBuf> {
    let project = projects::get_project(store, project_name).await?;
    let path = project
        .path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| report!("project '{project_name}' has no workspace path"))?;
    existing_workspace_path(path)
}

pub(crate) fn existing_workspace_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    let canonical = path
        .canonicalize()
        .context_with(|| format!("workspace path '{}' does not exist", path.display()))?;
    if !canonical.is_dir() {
        bail!(
            "workspace path '{}' is not a directory",
            canonical.display()
        );
    }
    Ok(canonical)
}

pub(crate) async fn open_workspace_path(
    target: WorkspaceOpenTarget,
    path: impl AsRef<Path>,
) -> Result<()> {
    let path = existing_workspace_path(path)?;
    let command = WorkspaceOpenConfig::from_env().command_for(target, &path)?;
    command.run().await
}

pub(crate) fn available_workspace_editors() -> Vec<WorkspaceEditorView> {
    WorkspaceOpenConfig::from_env().available_editors()
}

fn folder_open_command(os: &str, path: &Path) -> Result<WorkspaceOpenCommand> {
    match os {
        "macos" => Ok(command("open", [path.as_os_str()])),
        "linux" => Ok(command("xdg-open", [path.as_os_str()])),
        "windows" => Ok(command("explorer", [path.as_os_str()])),
        other => bail!("folder opening is not supported on {other}"),
    }
}

fn editor_open_command(
    config: &WorkspaceOpenConfig,
    target: WorkspaceOpenTarget,
    path: &Path,
) -> Result<WorkspaceOpenCommand> {
    match target {
        WorkspaceOpenTarget::RustRover if !config.rustrover_available() => {
            bail!("RustRover is not available on this system")
        }
        WorkspaceOpenTarget::VsCode if !config.vscode_available() => {
            bail!("VS Code is not available on this system")
        }
        _ => {}
    }

    match target {
        WorkspaceOpenTarget::RustRover => rustrover_open_command(config, path),
        WorkspaceOpenTarget::VsCode => vscode_open_command(config, path),
        WorkspaceOpenTarget::Folder => folder_open_command(&config.os, path),
    }
}

fn rustrover_open_command(
    config: &WorkspaceOpenConfig,
    path: &Path,
) -> Result<WorkspaceOpenCommand> {
    match config.os.as_str() {
        "macos" if macos_application_exists("RustRover", config.home.as_deref()) => Ok(command(
            "open",
            [OsStr::new("-a"), OsStr::new("RustRover"), path.as_os_str()],
        )),
        "windows" => {
            let program = first_program_in_path(["rustrover64.exe"], config.path_env.as_deref())
                .unwrap_or("rustrover64.exe");
            Ok(command(program, [path.as_os_str()]))
        }
        _ => Ok(command("rustrover", [path.as_os_str()])),
    }
}

fn vscode_open_command(config: &WorkspaceOpenConfig, path: &Path) -> Result<WorkspaceOpenCommand> {
    match config.os.as_str() {
        "macos" if macos_application_exists("Visual Studio Code", config.home.as_deref()) => {
            Ok(command(
                "open",
                [
                    OsStr::new("-a"),
                    OsStr::new("Visual Studio Code"),
                    path.as_os_str(),
                ],
            ))
        }
        "windows" => {
            let program =
                first_program_in_path(["code.cmd", "code.exe"], config.path_env.as_deref())
                    .unwrap_or("code.cmd");
            Ok(command(program, [path.as_os_str()]))
        }
        _ => Ok(command("code", [path.as_os_str()])),
    }
}

fn command<const N: usize>(program: impl AsRef<OsStr>, args: [&OsStr; N]) -> WorkspaceOpenCommand {
    WorkspaceOpenCommand {
        program: program.as_ref().to_owned(),
        args: args.into_iter().map(OsStr::to_owned).collect(),
    }
}

fn workspace_editor_target_value(target: WorkspaceOpenTarget) -> &'static str {
    match target {
        WorkspaceOpenTarget::Folder => "folder",
        WorkspaceOpenTarget::RustRover => "rustrover",
        WorkspaceOpenTarget::VsCode => "vscode",
    }
}

fn program_in_path(program: &str, path_env: Option<&OsStr>) -> bool {
    path_env.is_some_and(|path_env| {
        env::split_paths(path_env).any(|directory| is_runnable_file(&directory.join(program)))
    })
}

fn first_program_in_path<'a, const N: usize>(
    programs: [&'a str; N],
    path_env: Option<&OsStr>,
) -> Option<&'a str> {
    programs
        .into_iter()
        .find(|program| program_in_path(program, path_env))
}

fn is_runnable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn macos_application_exists(app_name: &str, home: Option<&OsStr>) -> bool {
    let bundle_name = format!("{app_name}.app");
    let system_app = Path::new("/Applications").join(&bundle_name);
    if system_app.is_dir() {
        return true;
    }
    home.is_some_and(|home| {
        PathBuf::from(home)
            .join("Applications")
            .join(bundle_name)
            .is_dir()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_path_program(directory: &Path, program: &str) {
        std::fs::write(directory.join(program), "").unwrap();
        #[cfg(unix)]
        {
            use std::{fs::Permissions, os::unix::fs::PermissionsExt};

            std::fs::set_permissions(directory.join(program), Permissions::from_mode(0o755))
                .unwrap();
        }
    }

    #[test]
    fn macos_rustrover_command_uses_open_application_when_bundle_exists() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("Applications/RustRover.app")).unwrap();
        let path = Path::new("/tmp/demo");
        let config = WorkspaceOpenConfig {
            os: "macos".to_owned(),
            path_env: None,
            home: Some(temp.path().as_os_str().to_owned()),
        };

        let command = config
            .command_for(WorkspaceOpenTarget::RustRover, path)
            .unwrap();

        assert_eq!(command.program, OsString::from("open"));
        assert_eq!(
            command.args,
            vec![
                OsString::from("-a"),
                OsString::from("RustRover"),
                OsString::from("/tmp/demo"),
            ]
        );
    }

    #[test]
    fn available_editors_are_limited_to_detected_targets() {
        let temp = tempfile::tempdir().unwrap();
        create_path_program(temp.path(), "code");
        let config = WorkspaceOpenConfig {
            os: "linux".to_owned(),
            path_env: Some(temp.path().as_os_str().to_owned()),
            home: None,
        };

        let editors = config.available_editors();

        assert_eq!(editors.len(), 1);
        assert_eq!(editors[0].target, "vscode");
        assert_eq!(editors[0].label, "VS Code");
    }

    #[test]
    fn linux_rustrover_command_uses_fixed_program() {
        let temp = tempfile::tempdir().unwrap();
        create_path_program(temp.path(), "rustrover");
        let path = Path::new("/tmp/demo");
        let config = WorkspaceOpenConfig {
            os: "linux".to_owned(),
            path_env: Some(temp.path().as_os_str().to_owned()),
            home: None,
        };

        let command = config
            .command_for(WorkspaceOpenTarget::RustRover, path)
            .unwrap();

        assert_eq!(command.program, OsString::from("rustrover"));
        assert_eq!(command.args, vec![OsString::from("/tmp/demo")]);
    }

    #[test]
    fn editor_command_rejects_unavailable_targets() {
        let path = Path::new("/tmp/demo");
        let config = WorkspaceOpenConfig {
            os: "linux".to_owned(),
            path_env: None,
            home: None,
        };

        let err = config
            .command_for(WorkspaceOpenTarget::RustRover, path)
            .unwrap_err()
            .to_string();

        assert!(err.contains("RustRover is not available"));
    }

    #[test]
    fn windows_vscode_command_uses_detected_program() {
        let temp = tempfile::tempdir().unwrap();
        create_path_program(temp.path(), "code.exe");
        let path = Path::new("C:/demo");
        let config = WorkspaceOpenConfig {
            os: "windows".to_owned(),
            path_env: Some(temp.path().as_os_str().to_owned()),
            home: None,
        };

        let command = config
            .command_for(WorkspaceOpenTarget::VsCode, path)
            .unwrap();

        assert_eq!(command.program, OsString::from("code.exe"));
        assert_eq!(command.args, vec![OsString::from("C:/demo")]);
    }

    #[test]
    fn linux_folder_command_uses_xdg_open() {
        let command = folder_open_command("linux", Path::new("/tmp/demo")).unwrap();

        assert_eq!(command.program, OsString::from("xdg-open"));
        assert_eq!(command.args, vec![OsString::from("/tmp/demo")]);
    }

    #[test]
    fn missing_workspace_path_is_rejected() {
        let err = existing_workspace_path("/definitely/not/a/dispatch/workspace")
            .unwrap_err()
            .to_string();

        assert!(err.contains("does not exist"));
    }
}
