//! Discovery and validation-first cleanup for oversized Codex log databases.

use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use rootcause::{Result, prelude::*};

use crate::shared::view_models::{
    CodexLogDatabaseView, CodexLogPurgeResultView, CodexLogStorageStatusView,
};

pub(crate) const CODEX_LOG_DATABASE_THRESHOLD_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_MANAGED_HOME_SCAN_ENTRIES: usize = 100_000;
const PROJECTS_DIR: &str = "projects";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DatabaseFileKind {
    Database,
    Wal,
    Shm,
}

#[derive(Debug)]
struct ValidatedFamily {
    files: Vec<(PathBuf, u64)>,
}

#[derive(Debug, Default)]
struct ScanBudget {
    entries: usize,
    exhausted: bool,
}

impl ScanBudget {
    fn count(&mut self, scan_errors: &mut Vec<String>) -> bool {
        if self.exhausted {
            return false;
        }
        self.entries += 1;
        if self.entries <= MAX_MANAGED_HOME_SCAN_ENTRIES {
            return true;
        }
        self.exhausted = true;
        scan_errors.push(format!(
            "Managed Codex log scan stopped after {MAX_MANAGED_HOME_SCAN_ENTRIES} directory entries."
        ));
        false
    }
}

pub(crate) fn scan_managed_codex_logs(codex_home: &Path) -> CodexLogStorageStatusView {
    scan_managed_codex_logs_with_threshold(codex_home, CODEX_LOG_DATABASE_THRESHOLD_BYTES)
}

fn scan_managed_codex_logs_with_threshold(
    codex_home: &Path,
    threshold_bytes: u64,
) -> CodexLogStorageStatusView {
    let mut families = BTreeMap::<PathBuf, u64>::new();
    let mut scan_errors = Vec::new();
    let mut budget = ScanBudget::default();

    scan_log_directory(
        codex_home,
        Path::new(""),
        &mut families,
        &mut scan_errors,
        &mut budget,
    );
    scan_project_directories(codex_home, &mut families, &mut scan_errors, &mut budget);

    let oversized_databases = families
        .into_iter()
        .filter(|(_, size_bytes)| *size_bytes > threshold_bytes)
        .map(|(relative_path, size_bytes)| CodexLogDatabaseView {
            relative_path: relative_path.to_string_lossy().into_owned(),
            size_bytes,
        })
        .collect();
    scan_errors.sort();

    CodexLogStorageStatusView {
        threshold_bytes,
        oversized_databases,
        scan_errors,
    }
}

fn scan_project_directories(
    codex_home: &Path,
    families: &mut BTreeMap<PathBuf, u64>,
    scan_errors: &mut Vec<String>,
    budget: &mut ScanBudget,
) {
    let projects_dir = codex_home.join(PROJECTS_DIR);
    let metadata = match fs::symlink_metadata(&projects_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => {
            push_scan_error(scan_errors, Path::new(PROJECTS_DIR), &error);
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if !metadata.is_dir() {
        scan_errors.push(format!(
            "Managed Codex log scan could not inspect `{PROJECTS_DIR}` because it is not a directory."
        ));
        return;
    }

    let entries = match fs::read_dir(&projects_dir) {
        Ok(entries) => entries,
        Err(error) => {
            push_scan_error(scan_errors, Path::new(PROJECTS_DIR), &error);
            return;
        }
    };
    for entry in entries {
        if !budget.count(scan_errors) {
            return;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                push_scan_error(scan_errors, Path::new(PROJECTS_DIR), &error);
                continue;
            }
        };
        let project_name = entry.file_name();
        let Some(project_id) = project_name.to_str() else {
            continue;
        };
        if project_id
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .is_none()
        {
            continue;
        }
        let relative_dir = Path::new(PROJECTS_DIR).join(project_id);
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                push_scan_error(scan_errors, &relative_dir, &error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        scan_log_directory(&entry.path(), &relative_dir, families, scan_errors, budget);
    }
}

fn scan_log_directory(
    directory: &Path,
    relative_dir: &Path,
    families: &mut BTreeMap<PathBuf, u64>,
    scan_errors: &mut Vec<String>,
    budget: &mut ScanBudget,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            push_scan_error(scan_errors, relative_dir, &error);
            return;
        }
    };
    for entry in entries {
        if !budget.count(scan_errors) {
            return;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                push_scan_error(scan_errors, relative_dir, &error);
                continue;
            }
        };
        let Some(file_name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let Some((database_file_name, _kind)) = log_database_file(&file_name) else {
            continue;
        };
        let relative_file = relative_dir.join(&file_name);
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                push_scan_error(scan_errors, &relative_file, &error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let relative_database = relative_dir.join(database_file_name);
        let current_size = families
            .get(&relative_database)
            .copied()
            .unwrap_or_default();
        match current_size.checked_add(metadata.len()) {
            Some(size) => {
                families.insert(relative_database, size);
            }
            None => {
                families.remove(&relative_database);
                scan_errors.push(format!(
                    "Managed Codex log scan overflowed while totaling `{}`.",
                    relative_database.display()
                ));
            }
        }
    }
}

fn push_scan_error(scan_errors: &mut Vec<String>, relative_path: &Path, error: &std::io::Error) {
    let path = if relative_path.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative_path.to_string_lossy().into_owned()
    };
    scan_errors.push(format!(
        "Managed Codex log scan could not inspect `{path}`: {error}."
    ));
}

fn log_database_file(file_name: &str) -> Option<(&str, DatabaseFileKind)> {
    let (database_file_name, kind) = if let Some(database) = file_name.strip_suffix("-wal") {
        (database, DatabaseFileKind::Wal)
    } else if let Some(database) = file_name.strip_suffix("-shm") {
        (database, DatabaseFileKind::Shm)
    } else {
        (file_name, DatabaseFileKind::Database)
    };
    (database_file_name.starts_with("logs_") && database_file_name.ends_with(".sqlite"))
        .then_some((database_file_name, kind))
}

pub(crate) fn purge_oversized_codex_logs(codex_home: &Path) -> Result<CodexLogPurgeResultView> {
    let status = scan_managed_codex_logs(codex_home);
    purge_scan_result(codex_home, &status)
}

fn purge_scan_result(
    codex_home: &Path,
    status: &CodexLogStorageStatusView,
) -> Result<CodexLogPurgeResultView> {
    if !status.scan_errors.is_empty() {
        bail!(
            "Codex log cleanup is disabled because the managed-home scan failed: {}",
            status.scan_errors.join(" ")
        );
    }

    let mut validated = Vec::new();
    for database in &status.oversized_databases {
        let relative_database = Path::new(&database.relative_path);
        validate_database_relative_path(relative_database)?;
        let mut files = Vec::new();
        let mut current_size = 0_u64;
        for suffix in ["-wal", "-shm", ""] {
            let relative_file =
                PathBuf::from(format!("{}{}", relative_database.to_string_lossy(), suffix));
            let path = codex_home.join(&relative_file);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        bail!(
                            "refusing to purge Codex log target `{}` because it is not a regular file",
                            relative_file.display()
                        );
                    }
                    current_size = current_size.checked_add(metadata.len()).ok_or_else(|| {
                        report!(
                            "Codex log family `{}` is too large to total safely",
                            relative_database.display()
                        )
                    })?;
                    files.push((path, metadata.len()));
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    Err(error).context_with(|| {
                        format!(
                            "failed to validate Codex log target {}",
                            relative_file.display()
                        )
                    })?;
                }
            }
        }
        if current_size > status.threshold_bytes {
            validated.push(ValidatedFamily { files });
        }
    }

    let mut result = CodexLogPurgeResultView::default();
    for family in validated {
        let mut removed_family = false;
        for (path, size_bytes) in family.files {
            match fs::remove_file(&path) {
                Ok(()) => {
                    removed_family = true;
                    result.reclaimed_bytes = result.reclaimed_bytes.saturating_add(size_bytes);
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    Err(error).context_with(|| {
                        format!("failed to remove Codex log file {}", path.display())
                    })?;
                }
            }
        }
        if removed_family {
            result.removed_database_count += 1;
        }
    }
    Ok(result)
}

fn validate_database_relative_path(relative_path: &Path) -> Result<()> {
    if relative_path.is_absolute() {
        bail!(
            "refusing absolute Codex log target `{}`",
            relative_path.display()
        );
    }
    let components = relative_path.components().collect::<Vec<_>>();
    let valid_shape = match components.as_slice() {
        [Component::Normal(file_name)] => file_name.to_str().is_some_and(|file_name| {
            log_database_file(file_name) == Some((file_name, DatabaseFileKind::Database))
        }),
        [
            Component::Normal(projects),
            Component::Normal(project_id),
            Component::Normal(file_name),
        ] => {
            projects == &std::ffi::OsStr::new(PROJECTS_DIR)
                && project_id
                    .to_str()
                    .and_then(|id| id.parse::<i64>().ok())
                    .is_some_and(|id| id > 0)
                && file_name.to_str().is_some_and(|file_name| {
                    log_database_file(file_name) == Some((file_name, DatabaseFileKind::Database))
                })
        }
        _ => false,
    };
    if !valid_shape {
        bail!(
            "refusing invalid managed Codex log target `{}`",
            relative_path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;

    fn sparse_file(path: &Path, size: u64) {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        file.set_len(size).unwrap();
    }

    #[test]
    fn scanner_groups_sidecars_and_uses_strict_threshold() {
        let home = TempDir::new().unwrap();
        sparse_file(&home.path().join("logs_exact.sqlite"), 80);
        sparse_file(&home.path().join("logs_exact.sqlite-wal"), 20);
        sparse_file(&home.path().join("logs_large.sqlite"), 80);
        sparse_file(&home.path().join("logs_large.sqlite-wal"), 20);
        sparse_file(&home.path().join("logs_large.sqlite-shm"), 1);

        let status = scan_managed_codex_logs_with_threshold(home.path(), 100);

        assert_that!(&(status.oversized_databases.len())).is_equal_to(1);
        assert_that!(&status.oversized_databases[0].relative_path).is_equal_to("logs_large.sqlite");
        assert_that!(&(status.oversized_databases[0].size_bytes)).is_equal_to(101);
    }

    #[test]
    fn exactly_one_gibibyte_is_not_oversized() {
        let home = TempDir::new().unwrap();
        sparse_file(
            &home.path().join("logs_exact.sqlite"),
            CODEX_LOG_DATABASE_THRESHOLD_BYTES,
        );
        sparse_file(
            &home.path().join("logs_over.sqlite"),
            CODEX_LOG_DATABASE_THRESHOLD_BYTES + 1,
        );

        let status = scan_managed_codex_logs(home.path());

        assert_that!(&(status.oversized_databases.len())).is_equal_to(1);
        assert_that!(&status.oversized_databases[0].relative_path).is_equal_to("logs_over.sqlite");
    }

    #[test]
    fn scanner_discovers_shared_and_project_logs_and_ignores_unmanaged_files() {
        let home = TempDir::new().unwrap();
        let project = home.path().join("projects/42");
        fs::create_dir_all(&project).unwrap();
        sparse_file(&home.path().join("logs_shared.sqlite"), 101);
        sparse_file(&project.join("logs_project.sqlite"), 101);
        sparse_file(&project.join("state_5.sqlite"), 200);
        sparse_file(&project.join("logs_small.sqlite"), 50);
        fs::write(project.join("codex.log"), b"text log").unwrap();
        fs::create_dir_all(project.join("sessions")).unwrap();
        fs::create_dir_all(home.path().join("projects/not-an-id")).unwrap();
        sparse_file(
            &home.path().join("projects/not-an-id/logs_ignored.sqlite"),
            200,
        );

        let status = scan_managed_codex_logs_with_threshold(home.path(), 100);
        let paths = status
            .oversized_databases
            .iter()
            .map(|database| database.relative_path.as_str())
            .collect::<Vec<_>>();

        assert_that!(&(paths.len())).is_equal_to(2);
        assert_that!(&(paths.contains(&"logs_shared.sqlite"))).is_true();
        assert_that!(&(paths.contains(&"projects/42/logs_project.sqlite"))).is_true();
        assert_that!(&(status.scan_errors.is_empty())).is_true();
    }

    #[cfg(unix)]
    #[test]
    fn scanner_ignores_symlinked_files_and_project_directories() {
        use std::os::unix::fs::symlink;

        let home = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        sparse_file(&outside.path().join("logs_outside.sqlite"), 200);
        symlink(
            outside.path().join("logs_outside.sqlite"),
            home.path().join("logs_link.sqlite"),
        )
        .unwrap();
        fs::create_dir(home.path().join("projects")).unwrap();
        symlink(outside.path(), home.path().join("projects/42")).unwrap();

        let status = scan_managed_codex_logs_with_threshold(home.path(), 100);

        assert_that!(&(status.oversized_databases.is_empty())).is_true();
        assert_that!(&(status.scan_errors.is_empty())).is_true();
    }

    #[test]
    fn scanner_reports_project_directory_errors_without_losing_shared_findings() {
        let home = TempDir::new().unwrap();
        sparse_file(&home.path().join("logs_shared.sqlite"), 101);
        fs::write(home.path().join("projects"), b"not a directory").unwrap();

        let status = scan_managed_codex_logs_with_threshold(home.path(), 100);

        assert_that!(&(status.oversized_databases.len())).is_equal_to(1);
        assert_that!(&(status.scan_errors.len())).is_equal_to(1);
        assert_that!(&status.scan_errors[0]).contains("not a directory");
    }

    #[test]
    fn purge_is_idempotent_and_preserves_smaller_and_unrelated_files() {
        let home = TempDir::new().unwrap();
        sparse_file(&home.path().join("logs_large.sqlite"), 80);
        sparse_file(&home.path().join("logs_large.sqlite-wal"), 21);
        sparse_file(&home.path().join("logs_small.sqlite"), 50);
        fs::write(home.path().join("session.log"), b"keep").unwrap();
        let status = scan_managed_codex_logs_with_threshold(home.path(), 100);

        let result = purge_scan_result(home.path(), &status).unwrap();

        assert_that!(&(result.removed_database_count)).is_equal_to(1);
        assert_that!(&(result.reclaimed_bytes)).is_equal_to(101);
        assert_that!(&(!home.path().join("logs_large.sqlite").exists())).is_true();
        assert_that!(&(!home.path().join("logs_large.sqlite-wal").exists())).is_true();
        assert_that!(&(home.path().join("logs_small.sqlite").exists())).is_true();
        assert_that!(&(home.path().join("session.log").exists())).is_true();

        let clean = purge_scan_result(
            home.path(),
            &scan_managed_codex_logs_with_threshold(home.path(), 100),
        )
        .unwrap();
        assert_that!(&(clean.removed_database_count)).is_equal_to(0);
        assert_that!(&(clean.reclaimed_bytes)).is_equal_to(0);
    }

    #[test]
    fn purge_validates_every_target_before_removing_any_file() {
        let home = TempDir::new().unwrap();
        sparse_file(&home.path().join("logs_valid.sqlite"), 101);
        let status = CodexLogStorageStatusView {
            threshold_bytes: 100,
            oversized_databases: vec![
                CodexLogDatabaseView {
                    relative_path: "logs_valid.sqlite".to_owned(),
                    size_bytes: 101,
                },
                CodexLogDatabaseView {
                    relative_path: "../logs_outside.sqlite".to_owned(),
                    size_bytes: 101,
                },
            ],
            scan_errors: Vec::new(),
        };

        let result = purge_scan_result(home.path(), &status);

        assert_that!(&(result.is_err())).is_true();
        assert_that!(&(home.path().join("logs_valid.sqlite").exists())).is_true();
    }

    #[test]
    fn purge_refuses_when_scan_errors_are_present() {
        let home = TempDir::new().unwrap();
        sparse_file(&home.path().join("logs_large.sqlite"), 101);
        let status = CodexLogStorageStatusView {
            threshold_bytes: 100,
            oversized_databases: vec![CodexLogDatabaseView {
                relative_path: "logs_large.sqlite".to_owned(),
                size_bytes: 101,
            }],
            scan_errors: vec!["scan failed".to_owned()],
        };

        let result = purge_scan_result(home.path(), &status);

        assert_that!(&(result.is_err())).is_true();
        assert_that!(&(home.path().join("logs_large.sqlite").exists())).is_true();
    }
}
