use rootcause::{Result, prelude::*};
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
