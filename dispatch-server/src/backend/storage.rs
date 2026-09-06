use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use rootcause::{Result, prelude::*};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::backend::migrations::Migrator;

#[derive(Clone)]
pub struct Store {
    db: Arc<DatabaseConnection>,
    path: Arc<PathBuf>,
    knowledge_jobs_lock: Arc<tokio::sync::Mutex<()>>,
    runtime_admission_lock: Arc<tokio::sync::Mutex<()>>,
    automation_production_lock: Arc<tokio::sync::Mutex<()>>,
}
impl Store {
    pub async fn open(path: PathBuf) -> Result<Self> {
        Self::open_with_connection_limit(path, None).await
    }

    #[cfg(test)]
    pub(crate) async fn open_with_max_connections(
        path: PathBuf,
        max_connections: u32,
    ) -> Result<Self> {
        Self::open_with_connection_limit(path, Some(max_connections)).await
    }

    async fn open_with_connection_limit(
        path: PathBuf,
        max_connections: Option<u32>,
    ) -> Result<Self> {
        let path = absolute_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context_with(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
        }

        let url = sqlite_url(&path);
        let mut options = ConnectOptions::new(url);
        if let Some(max_connections) = max_connections {
            options.max_connections(max_connections);
        }
        let db = Database::connect(options)
            .await
            .context_with(|| format!("failed to open database {}", path.display()))?;
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .context("failed to enable SQLite foreign keys")?;

        Migrator::up(&db, None)
            .await
            .context("failed to apply database migrations")?;

        Ok(Self {
            db: Arc::new(db),
            path: Arc::new(path),
            knowledge_jobs_lock: Arc::new(tokio::sync::Mutex::new(())),
            runtime_admission_lock: Arc::new(tokio::sync::Mutex::new(())),
            automation_production_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(crate) async fn lock_knowledge_jobs(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.knowledge_jobs_lock.lock().await
    }
    pub(crate) async fn lock_runtime_admission(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.runtime_admission_lock.lock().await
    }

    pub fn db(&self) -> Arc<DatabaseConnection> {
        self.db.clone()
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref().as_path()
    }

    /// Serializes the producer deduplication read and item creation transaction.
    ///
    /// Dispatch has one owning server process for a database. SQLite deferred transactions do not
    /// acquire a write lock until the first write, so concurrent producer evaluations otherwise
    /// could both observe no unfinished item before either inserts its origin row.
    pub(crate) async fn lock_automation_production(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.automation_production_lock.lock().await
    }
}

pub fn default_database_path() -> PathBuf {
    dispatch_home_dir().join("dispatch.sqlite3")
}

pub fn dispatch_home_dir() -> PathBuf {
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".dispatch");
    }

    PathBuf::from(".dispatch")
}

pub fn utc_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(env::current_dir()
        .context("failed to read current directory for database path")?
        .join(path))
}
