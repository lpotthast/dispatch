//! Project-root ignore controls, evaluated independently for each family before opening files.
use dispatch_types::knowledge::KnowledgeDiagnostic;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rootcause::{Result, prelude::*};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(super) struct Discovery {
    pub files: Vec<PathBuf>,
    pub diagnostics: Vec<KnowledgeDiagnostic>,
    pub controls: Vec<(PathBuf, String)>,
}

#[derive(Clone, Default)]
struct Rules {
    git: Vec<Gitignore>,
    dispatch: Vec<Gitignore>,
}

impl Rules {
    fn excludes(&self, path: &Path, is_dir: bool) -> bool {
        [&self.git, &self.dispatch].iter().any(|family| {
            family
                .iter()
                .rev()
                .find_map(|rules| {
                    let matched = rules.matched(path, is_dir);
                    (!matched.is_none()).then(|| matched.is_ignore())
                })
                .unwrap_or(false)
        })
    }

    fn extend(&mut self, dir: &Path, output: &mut Discovery) -> Result<()> {
        for (name, family) in [
            (".gitignore", &mut self.git),
            (".dispatchignore", &mut self.dispatch),
        ] {
            let path = dir.join(name);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(report!(error)
                        .context("cannot inspect ignore control")
                        .into_dynamic());
                }
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                bail!("ignore control {} must be a regular file", path.display());
            }
            let text = fs::read_to_string(&path)
                .context_with(|| format!("cannot read {}", path.display()))?;
            output.controls.push((
                path.clone(),
                format!("{:x}", Sha256::digest(text.as_bytes())),
            ));
            let mut builder = GitignoreBuilder::new(dir);
            for line in text.lines() {
                builder.add_line(Some(path.clone()), line)?;
            }
            family.push(builder.build()?);
        }
        Ok(())
    }
}

impl Discovery {
    pub fn read(workspace: &Path, directory: &str) -> Self {
        let mut output = Self::default();
        let mut rules = Rules::default();
        let mut current = workspace.to_path_buf();
        for part in Path::new(directory).components() {
            if let Err(error) = rules.extend(&current, &mut output) {
                output.problem(&current, error.to_string());
                return output;
            }
            current.push(part);
            if rules.excludes(&current, true) {
                return output;
            }
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return output,
                _ => {
                    output.problem(
                        &current,
                        "knowledge directory is not an accessible regular directory".into(),
                    );
                    return output;
                }
            }
        }
        output.walk(&current, &rules);
        output.files.sort();
        output.controls.sort();
        output
    }

    /// Validate a proposed file as well as existing files without temporarily creating it.
    /// Both ignore families and every ancestor must allow the path before a write starts.
    pub(super) fn check_file(workspace: &Path, directory: &str, relative: &str) -> Result<()> {
        let mut rules = Rules::default();
        let mut output = Self::default();
        let mut current = workspace.to_path_buf();
        let path = Path::new(directory).join(relative);
        let parts: Vec<_> = path.components().collect();
        for (position, part) in parts.iter().enumerate() {
            rules.extend(&current, &mut output)?;
            current.push(part);
            let is_dir = position + 1 < parts.len();
            if rules.excludes(&current, is_dir) {
                bail!("document is excluded by .gitignore or .dispatchignore");
            }
            match fs::symlink_metadata(&current) {
                Ok(meta)
                    if !meta.file_type().is_symlink()
                        && if is_dir {
                            meta.is_dir()
                        } else {
                            meta.is_file()
                        } => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => bail!("document path must contain only regular directories and files"),
            }
        }
        Ok(())
    }

    fn walk(&mut self, dir: &Path, inherited: &Rules) {
        let mut rules = inherited.clone();
        if let Err(error) = rules.extend(dir, self) {
            self.problem(dir, error.to_string());
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                self.problem(dir, error.to_string());
                return;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    self.problem(dir, error.to_string());
                    continue;
                }
            };
            let path = entry.path();
            if matches!(
                entry.file_name().to_str(),
                Some(".git" | ".dispatch" | ".knowledge")
            ) {
                continue;
            }
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    self.problem(&path, error.to_string());
                    continue;
                }
            };
            if kind.is_symlink() || rules.excludes(&path, kind.is_dir()) {
                continue;
            }
            if kind.is_dir() {
                self.walk(&path, &rules);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                self.files.push(path);
            }
        }
    }

    fn problem(&mut self, path: &Path, message: String) {
        self.diagnostics.push(KnowledgeDiagnostic {
            path: path.to_string_lossy().into_owned(),
            code: "discovery_failed".into(),
            message,
        });
    }
}
