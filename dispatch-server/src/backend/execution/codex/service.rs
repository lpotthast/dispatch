use super::{model::SharedCodexStatus, runtime::*};
use crate::backend::{
    events::UiEventBus,
    execution::codex::logs as codex_log_storage,
    execution::sessions::{CodexMaintenanceAdmissionRejection, ProcessSessionRegistry},
    execution::tools::service::ToolService,
    storage::utc_now,
};
use crate::shared::view_models::{
    AgentToolName, CodexAppServerStatusView, CodexLogPurgeResultView,
};
use rootcause::{Result, prelude::*};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Mutex, RwLock},
    time::{Instant, timeout},
};
/// Deduplicates detailed status probes across `/system` page loads, live events, and browser tabs.
#[derive(Clone)]
pub(crate) struct CodexService {
    home: PathBuf,
    events: UiEventBus,
    sessions: ProcessSessionRegistry,
    pub(super) operation: Arc<Mutex<()>>,
    tools: Arc<crate::backend::execution::tools::service::ToolService>,
    last_detailed_refresh: Arc<Mutex<Option<DetailedStatusCache>>>,
}

#[derive(Clone)]
struct DetailedStatusCache {
    refreshed_at: Instant,
    status: CodexAppServerStatusView,
}

impl CodexService {
    pub(crate) fn new(
        tools: Arc<ToolService>,
        home: PathBuf,
        events: UiEventBus,
        sessions: ProcessSessionRegistry,
    ) -> Self {
        Self {
            home,
            events,
            sessions,
            operation: Default::default(),
            tools,
            last_detailed_refresh: Default::default(),
        }
    }
    pub(crate) async fn refresh_if_stale(
        &self,
        status: &RwLock<CodexAppServerStatusView>,
        minimum_age: Duration,
    ) -> CodexAppServerStatusView {
        self.refresh(status, Some(minimum_age)).await
    }

    pub(crate) async fn refresh_now(
        &self,
        status: &RwLock<CodexAppServerStatusView>,
    ) -> CodexAppServerStatusView {
        let refreshed = self.refresh(status, None).await;
        self.events.publish_codex_status_changed();
        refreshed
    }

    pub(crate) async fn store_detailed(
        &self,
        shared: &RwLock<CodexAppServerStatusView>,
        status: CodexAppServerStatusView,
    ) {
        let mut last_detailed_refresh = self.last_detailed_refresh.lock().await;
        *shared.write().await = status.clone();
        *last_detailed_refresh = Some(DetailedStatusCache {
            refreshed_at: Instant::now(),
            status,
        });
    }

    async fn refresh(
        &self,
        status: &RwLock<CodexAppServerStatusView>,
        minimum_age: Option<Duration>,
    ) -> CodexAppServerStatusView {
        let mut last_detailed_refresh = self.last_detailed_refresh.lock().await;
        if let Some(cached) = last_detailed_refresh.as_ref()
            && minimum_age.is_some_and(|minimum_age| cached.refreshed_at.elapsed() < minimum_age)
        {
            return cached.status.clone();
        }

        let refreshed = self.status().await;
        *status.write().await = refreshed.clone();
        *last_detailed_refresh = Some(DetailedStatusCache {
            refreshed_at: Instant::now(),
            status: refreshed.clone(),
        });
        refreshed
    }
}

impl CodexService {
    pub(crate) async fn publish_snapshot(
        &self,
        shared: &SharedCodexStatus,
        status: CodexAppServerStatusView,
    ) {
        *shared.write().await = status;
        self.events.publish_codex_status_changed();
    }
    /// Probes the configured Codex binary and returns its current automation readiness.
    ///
    /// Probe failures are represented in the returned view so status pages can render them directly.
    /// This function does not return an error.
    pub async fn status(&self) -> CodexAppServerStatusView {
        self.status_with_probe(StatusProbe::Detailed).await
    }
    /// Performs the minimal startup or pre-run readiness probe.
    ///
    /// Unlike [`Self::status`], this omits the optional account token-activity request.
    pub async fn readiness(&self) -> CodexAppServerStatusView {
        self.status_with_probe(StatusProbe::Readiness).await
    }
    async fn status_with_probe(&self, probe: StatusProbe) -> CodexAppServerStatusView {
        let _operation = self.operation.lock().await;
        let checked_at = utc_now();
        if let Err(err) = ensure_codex_home_at(&self.home) {
            let log_storage = codex_log_storage::scan_managed_codex_logs(&self.home);
            return with_log_storage(
                unavailable_status(
                    checked_at,
                    format!("Codex app-server is unavailable: {err:#}"),
                ),
                log_storage,
            );
        }
        let log_storage = codex_log_storage::scan_managed_codex_logs(&self.home);
        let status = match self.tools.resolve(AgentToolName::Codex)
        .await
        .context_with(|| {
            format!(
                "Dispatch cannot start Codex automation because Codex is not configured or discoverable. {CODEX_INSTALL_PROMPT}"
            )
        }) {
        Ok(path) => app_server_status_for_binary_unlocked(&self.home, &path, checked_at, probe).await,
        Err(err) => unavailable_status(
            checked_at,
            format!("Codex app-server is unavailable: {err:#}"),
        ),
    };
        with_log_storage(status, log_storage)
    }
    /// Logs out the account in Dispatch's managed Codex home and returns the refreshed status.
    ///
    /// # Errors
    ///
    /// Returns an error when the managed home or Codex binary is unavailable, the app-server rejects
    /// the logout, or the operation exceeds the status timeout.
    async fn logout_inner(&self) -> Result<CodexAppServerStatusView> {
        let _operation = self.operation.lock().await;
        ensure_codex_home_at(&self.home)?;
        let codex_binary = self.tools.resolve(AgentToolName::Codex)
        .await
        .context_with(|| {
            format!(
                "Dispatch cannot log out of Codex because Codex is not configured or discoverable. {CODEX_INSTALL_PROMPT}"
            )
        })?;
        timeout(STATUS_TIMEOUT, async {
            let app_server = spawn_initialized_app_server(&self.home, &codex_binary).await?;
            app_server
                .client()
                .account_logout()
                .await
                .context("Codex app-server rejected logout")?;
            let status = inspect_initialized_client(
                &self.home,
                &codex_binary,
                utc_now(),
                app_server.client(),
                StatusProbe::Detailed,
            )
            .await;
            app_server.shutdown().await?;
            Ok::<_, Report>(with_log_storage(
                status,
                codex_log_storage::scan_managed_codex_logs(&self.home),
            ))
        })
        .await
        .context("timed out while logging out of Codex")?
    }
    /// Performs the minimal pre-run readiness probe for an already resolved Codex binary.
    ///
    /// Failures are represented in the returned status so callers can publish the exact pre-run
    /// snapshot before deciding whether automation may continue.
    pub(crate) async fn readiness_for_binary(
        &self,
        codex_binary: &Path,
    ) -> CodexAppServerStatusView {
        let _operation = self.operation.lock().await;
        let checked_at = utc_now();
        if let Err(error) = ensure_codex_home_at(&self.home) {
            return with_log_storage(
                unavailable_status(
                    checked_at,
                    format!("Codex app-server is unavailable: {error:#}"),
                ),
                codex_log_storage::scan_managed_codex_logs(&self.home),
            );
        }
        let log_storage = codex_log_storage::scan_managed_codex_logs(&self.home);
        let status = app_server_status_for_binary_unlocked(
            &self.home,
            codex_binary,
            checked_at,
            StatusProbe::Readiness,
        )
        .await;
        with_log_storage(status, log_storage)
    }
    pub(super) async fn purge_logs_inner(&self) -> Result<CodexLogPurgeResultView> {
        let _admission = match self.sessions.begin_codex_maintenance() {
            Ok(admission) => admission,
            Err(CodexMaintenanceAdmissionRejection::ActiveSessions(count)) => {
                bail!(
                    "Cannot purge Codex logs while {count} Codex session{} active.",
                    if count == 1 { " is" } else { "s are" }
                );
            }
            Err(CodexMaintenanceAdmissionRejection::InProgress) => {
                bail!("Codex managed-home maintenance is already in progress.");
            }
        };
        let _operation = self.operation.lock().await;
        codex_log_storage::purge_oversized_codex_logs(&self.home)
    }
    pub(crate) async fn discover_tools(
        &self,
        shared: &RwLock<CodexAppServerStatusView>,
    ) -> Result<()> {
        self.tools.discover().await?;
        self.refresh_now(shared).await;
        Ok(())
    }
    pub(crate) async fn logout(&self, shared: &RwLock<CodexAppServerStatusView>) -> Result<()> {
        let status = self.logout_inner().await?;
        self.store_detailed(shared, status).await;
        self.events.publish_codex_status_changed();
        Ok(())
    }
    pub(crate) async fn purge_logs(
        &self,
        shared: &RwLock<CodexAppServerStatusView>,
    ) -> Result<CodexLogPurgeResultView> {
        let outcome = self.purge_logs_inner().await;
        self.refresh_now(shared).await;
        outcome
    }
    pub(crate) fn prepare_project_home(
        &self,
        settings: &crate::shared::view_models::ProjectSettingsView,
    ) -> Result<PathBuf> {
        ensure_isolated_codex_home(
            &self.home,
            settings,
            &self
                .home
                .join("projects")
                .join(settings.project_id.to_string()),
        )
    }
    pub(crate) fn prepare_isolated_home(
        &self,
        settings: &crate::shared::view_models::ProjectSettingsView,
        home: &Path,
    ) -> Result<PathBuf> {
        ensure_isolated_codex_home(&self.home, settings, home)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub(crate) fn service(
        store: &crate::backend::storage::Store,
    ) -> std::sync::Arc<super::CodexService> {
        let events = crate::backend::events::UiEventBus::new();
        std::sync::Arc::new(super::CodexService::new(
            crate::backend::execution::tools::tests::service(store),
            store.path().parent().unwrap().join("codex-home"),
            events.clone(),
            crate::backend::execution::sessions::ProcessSessionRegistry::new(events),
        ))
    }
    use super::*;
    use crate::backend::{execution::sessions::ProcessSessionStart, storage::Store};
    use crate::shared::view_models::UiEvent;
    use assertr::prelude::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn maintenance_locks_status_caches_and_events_belong_to_the_instance() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        let events_a = UiEventBus::new();
        let events_b = UiEventBus::new();
        let mut receiver_a = events_a.subscribe();
        let mut receiver_b = events_b.subscribe();
        let tools = crate::backend::execution::tools::tests::service(&store);
        let home_a = temp.path().join("a");
        let home_b = temp.path().join("b");
        std::fs::create_dir_all(&home_a).unwrap();
        std::fs::create_dir_all(&home_b).unwrap();
        let a = CodexService::new(
            tools.clone(),
            home_a,
            events_a.clone(),
            ProcessSessionRegistry::new(events_a),
        );
        let b = CodexService::new(
            tools,
            home_b,
            events_b.clone(),
            ProcessSessionRegistry::new(events_b),
        );
        let shared_a = Arc::new(RwLock::new(CodexAppServerStatusView::default()));
        let shared_b = Arc::new(RwLock::new(CodexAppServerStatusView::default()));
        a.store_detailed(
            &shared_a,
            CodexAppServerStatusView {
                checked_at: "a".into(),
                ..Default::default()
            },
        )
        .await;
        b.store_detailed(
            &shared_b,
            CodexAppServerStatusView {
                checked_at: "b".into(),
                ..Default::default()
            },
        )
        .await;
        let _operation = a.operation.lock().await;
        timeout(Duration::from_secs(1), b.purge_logs_inner())
            .await
            .unwrap()
            .unwrap();
        assert_that!(
            &a.refresh_if_stale(&shared_a, Duration::from_secs(60))
                .await
                .checked_at
        )
        .is_equal_to("a");
        assert_that!(
            &b.refresh_if_stale(&shared_b, Duration::from_secs(60))
                .await
                .checked_at
        )
        .is_equal_to("b");
        a.publish_snapshot(
            &shared_a,
            CodexAppServerStatusView {
                checked_at: "a-new".into(),
                ..Default::default()
            },
        )
        .await;
        assert_that!(&matches!(
            receiver_a.try_recv().unwrap(),
            UiEvent::CodexStatusChanged { .. }
        ))
        .is_true();
        assert_that!(&receiver_b.try_recv().is_err()).is_true();
        assert_that!(&shared_b.read().await.checked_at).is_equal_to("b");
    }

    #[tokio::test]
    async fn refused_purge_still_forces_detailed_refresh_and_notifies() {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        let tools = crate::backend::execution::tools::tests::service(&store);
        tools
            .set_path(AgentToolName::Codex, temp.path().join("missing-codex"))
            .await
            .unwrap();
        let events = UiEventBus::new();
        let sessions = ProcessSessionRegistry::new(events.clone());
        let _active = sessions.begin(ProcessSessionStart {
            run_id: 7,
            project_id: 1,
            project_name: "demo".into(),
            tool_name: "codex".into(),
            command: String::new(),
            working_dir: String::new(),
        });
        let codex = CodexService::new(
            tools,
            temp.path().join("managed-home"),
            events.clone(),
            sessions,
        );
        let shared = RwLock::new(CodexAppServerStatusView::default());
        codex
            .store_detailed(
                &shared,
                CodexAppServerStatusView {
                    available: true,
                    usable: true,
                    checked_at: "cached".into(),
                    ..Default::default()
                },
            )
            .await;
        let mut receiver = events.subscribe();
        let result = codex.purge_logs(&shared).await;
        assert_that!(&result.is_err()).is_true();
        let status = shared.read().await.clone();
        assert_that!(&status.available).is_false();
        assert_that!(&status.checked_at).is_not_equal_to("cached");
        assert_that!(&matches!(
            receiver.try_recv().unwrap(),
            UiEvent::CodexStatusChanged { .. }
        ))
        .is_true();
        assert_that!(&receiver.try_recv().is_err()).is_true();
    }
}
