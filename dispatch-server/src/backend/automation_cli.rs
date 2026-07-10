use std::{collections::HashSet, path::PathBuf};

use rootcause::{Result, prelude::*};

pub(crate) fn dispatch_cli_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("DISPATCH_CLI_PATH")
        .or_else(|| std::env::var_os("DISPATCH_CLI"))
        .map(PathBuf::from)
    {
        return ensure_dispatch_cli_path(path);
    }

    let dev_script_search = find_dev_dispatch_cli();
    if let Some(dev_script) = dev_script_search.path {
        return ensure_dispatch_cli_path(dev_script);
    }

    let searched = dev_script_search
        .searched
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "Dispatch agent-facing CLI is not configured; set DISPATCH_CLI_PATH or create dev-bin/dispatch (searched: {searched})"
    )
}

#[derive(Debug)]
struct DevDispatchCliSearch {
    path: Option<PathBuf>,
    searched: Vec<PathBuf>,
}

fn find_dev_dispatch_cli() -> DevDispatchCliSearch {
    let mut roots = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        roots.push(parent.to_path_buf());
    }
    find_dev_dispatch_cli_from_roots(roots)
}

fn find_dev_dispatch_cli_from_roots(
    roots: impl IntoIterator<Item = PathBuf>,
) -> DevDispatchCliSearch {
    let mut seen = HashSet::new();
    let mut searched = Vec::new();
    for root in roots {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join("dev-bin/dispatch");
            if !seen.insert(candidate.clone()) {
                continue;
            }
            if candidate.is_file() {
                return DevDispatchCliSearch {
                    path: Some(candidate),
                    searched,
                };
            }
            searched.push(candidate);
        }
    }
    DevDispatchCliSearch {
        path: None,
        searched,
    }
}

fn ensure_dispatch_cli_path(path: PathBuf) -> Result<PathBuf> {
    if !path.is_file() {
        bail!(
            "Dispatch agent-facing CLI path '{}' does not exist or is not a file",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn dev_dispatch_cli_search_walks_up_to_repo_root() {
        let temp = TempDir::new().unwrap();
        let shim = temp.path().join("dev-bin/dispatch");
        fs::create_dir_all(shim.parent().unwrap()).unwrap();
        fs::write(&shim, "#!/usr/bin/env sh\n").unwrap();
        let server_workdir = temp.path().join("dispatch-server/target/debug");
        fs::create_dir_all(&server_workdir).unwrap();

        let search = find_dev_dispatch_cli_from_roots([server_workdir]);

        assert_eq!(search.path.as_deref(), Some(shim.as_path()));
    }
}
