use std::{
    env, fs,
    path::{Path, PathBuf},
};
pub(crate) struct ToolDiscovery {
    path: Option<std::ffi::OsString>,
}
impl ToolDiscovery {
    pub(crate) fn new(path: Option<std::ffi::OsString>) -> Self {
        Self { path }
    }
    pub(crate) fn find(&self, name: &str) -> Option<PathBuf> {
        find_executable_in_path_var(name, self.path.as_deref()?)
    }
}
fn find_executable_in_path_var(name: &str, path_var: &std::ffi::OsStr) -> Option<PathBuf> {
    env::split_paths(path_var).find_map(|directory| {
        let candidate = directory.join(name);
        if is_executable_file(&candidate) {
            Some(candidate)
        } else {
            None
        }
    })
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use tempfile::TempDir;
    #[test]
    fn executable_lookup_uses_path_order() {
        let temp = TempDir::new().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let tool = bin.join("codex");
        fs::write(&tool, "#!/bin/sh\n").unwrap();

        let found = find_executable_in_path_var("codex", bin.as_os_str()).unwrap();

        assert_that!(&(found)).is_equal_to(tool);
    }
}
