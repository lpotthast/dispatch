use rootcause::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub(crate) struct PassFiles;
pub(crate) struct PassArtifacts {
    pub(crate) directory: PathBuf,
    pub(crate) developer: PathBuf,
    pub(crate) user: PathBuf,
    pub(crate) log: PathBuf,
    pub(crate) stderr: PathBuf,
}
impl PassFiles {
    pub(crate) fn prepare(
        &self,
        job: &Path,
        run: i64,
        instructions: &str,
        prompt: &str,
    ) -> Result<PassArtifacts> {
        let directory = job.join("runs");
        fs::create_dir_all(&directory)?;
        let developer = directory.join(format!("run-{run}.developer-instructions.md"));
        let user = directory.join(format!("run-{run}.user-prompt.md"));
        let log = directory.join(format!("run-{run}.output.json"));
        let stderr = directory.join(format!("run-{run}.codex-stderr.log"));
        fs::write(&stderr, "")?;
        fs::write(&developer, instructions)?;
        fs::write(&user, prompt)?;
        Ok(PassArtifacts {
            directory,
            developer,
            user,
            log,
            stderr,
        })
    }
}
