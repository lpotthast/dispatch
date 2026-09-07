//! Checked ordinary-file saves. No metadata sidecars or alternative content authority.
use super::{discovery::Discovery, policy::normalize_knowledge_directory};
use dispatch_types::knowledge::{KnowledgeSaveRequest, KnowledgeSaveResult};
use rootcause::{Result, prelude::*};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Serializes this application's ordinary saves and journaled multi-file publications.
#[derive(Default)]
pub(crate) struct DocumentWriter {
    writes: Mutex<()>,
}
impl DocumentWriter {
    pub(crate) fn with_write<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let _write = self
            .writes
            .lock()
            .map_err(|_| report!("knowledge writer is unavailable"))?;
        operation()
    }
    pub(crate) fn save(
        &self,
        workspace: &Path,
        directory: &str,
        request: &KnowledgeSaveRequest,
    ) -> Result<KnowledgeSaveResult> {
        self.with_write(|| save_file_locked(workspace, directory, request))
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn check_current(path: &Path, expected: &Option<String>) -> Result<()> {
    match (fs::read(path), expected) {
        (Ok(bytes), Some(expected)) if fingerprint(&bytes) == *expected => Ok(()),
        (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(_), None) => bail!("a document already exists at this path; choose another path"),
        (Ok(_), Some(_)) => bail!(
            "document changed on disk; your draft is preserved. Compare it with current content before saving"
        ),
        (Err(error), _) => Err(report!(error)
            .context("cannot read the document originally opened")
            .into_dynamic()),
    }
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn save_file_locked(
    workspace: &Path,
    directory: &str,
    request: &KnowledgeSaveRequest,
) -> Result<KnowledgeSaveResult> {
    normalize_knowledge_directory(directory)?;
    normalize_knowledge_directory(&request.path)?;
    if !request.path.ends_with(".md") || request.markdown.len() > 4 * 1024 * 1024 {
        bail!("save requires a Markdown path and at most 4 MiB of UTF-8 content");
    }
    let workspace = workspace
        .canonicalize()
        .context("cannot open registered working copy")?;
    Discovery::check_file(&workspace, directory, &request.path)?;
    let destination = workspace.join(directory).join(&request.path);
    check_current(&destination, &request.expected_fingerprint)?;
    let parent = destination.parent().expect("validated document parent");
    fs::create_dir_all(parent)?;
    Discovery::check_file(&workspace, directory, &request.path)?;
    let temporary =
        TemporaryFile(parent.join(format!(".dispatch-edit-{}.tmp", uuid::Uuid::new_v4())));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)?;
    if let Ok(metadata) = fs::metadata(&destination) {
        file.set_permissions(metadata.permissions())?;
    }
    file.write_all(request.markdown.as_bytes())?;
    file.sync_all()?;
    // Ignore edits and file edits made during preparation invalidate the write, too.
    Discovery::check_file(&workspace, directory, &request.path)?;
    check_current(&destination, &request.expected_fingerprint)?;
    if request.expected_fingerprint.is_none() {
        // A new path must not clobber another creator between the check and publication.
        fs::hard_link(&temporary.0, &destination)?;
    } else {
        fs::rename(&temporary.0, &destination)?;
    }
    Ok(KnowledgeSaveResult {
        fingerprint: fingerprint(request.markdown.as_bytes()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    #[tokio::test]
    async fn ordinary_saves_and_publication_share_one_writer_without_global_coordination() {
        let writer = std::sync::Arc::new(DocumentWriter::default());
        let held = writer.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let holder = tokio::task::spawn_blocking(move || {
            held.with_write(|| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        entered_rx.await.unwrap();
        let other = DocumentWriter::default();
        assert_that!(&other.with_write(|| Ok(42)).unwrap()).is_equal_to(42);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let mut save = tokio::task::spawn_blocking(move || {
            started_tx.send(()).unwrap();
            writer.save(
                &path,
                "knowledge",
                &KnowledgeSaveRequest {
                    path: "README.md".into(),
                    expected_fingerprint: None,
                    markdown: "# Current files".into(),
                },
            )
        });
        started_rx.await.unwrap();
        assert_that!(
            &tokio::time::timeout(std::time::Duration::from_millis(30), &mut save)
                .await
                .is_err()
        )
        .is_true();
        release_tx.send(()).unwrap();
        holder.await.unwrap().unwrap();
        save.await.unwrap().unwrap();
        assert_that!(&fs::read_to_string(temp.path().join("knowledge/README.md")).unwrap())
            .is_equal_to("# Current files");
    }
}
