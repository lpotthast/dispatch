#[cfg(feature = "ssr")]
use crate::backend::{app_state, automation, page_data};
use crate::frontend::{
    pages::{RunLogPage, RunsSection},
    services::{
        cache::{LocalStorageCache, QueryCache},
        request::{ServiceFuture, ServiceRequest},
    },
};
use crate::shared::view_models::RunLogView;
#[cfg(feature = "ssr")]
use dispatch_types::AgentRunStatus;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct RunService {
    load_section: ServiceRequest<String, RunsSection>,
    load_detail: ServiceRequest<(String, i64), RunLogView>,
    load_log: ServiceRequest<(Option<String>, Option<i64>), RunLogPage>,
    cancel_run: ServiceRequest<(String, i64), ()>,
    section_cache: Option<LocalStorageCache<RunsSection>>,
    detail_cache: Option<QueryCache<RunLogView>>,
    log_cache: Option<QueryCache<RunLogPage>>,
}

impl RunService {
    pub(crate) fn new(
        load_section: impl Fn(String) -> ServiceFuture<RunsSection> + Send + Sync + 'static,
        load_detail: impl Fn((String, i64)) -> ServiceFuture<RunLogView> + Send + Sync + 'static,
        load_log: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<RunLogPage>
        + Send
        + Sync
        + 'static,
        cancel_run: impl Fn((String, i64)) -> ServiceFuture<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_section: ServiceRequest::new(load_section),
            load_detail: ServiceRequest::new(load_detail),
            load_log: ServiceRequest::new(load_log),
            cancel_run: ServiceRequest::new(cancel_run),
            section_cache: None,
            detail_cache: None,
            log_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |project| Box::pin(load_runs_section(project)),
            |(project, run_id)| Box::pin(load_run_detail(project, run_id)),
            |(project, run_id)| Box::pin(load_run_log_page(project, run_id)),
            |(project, run_id)| Box::pin(cancel_run(project, run_id)),
        );
        service.section_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.runs-section.v2",
        ));
        // Active output grows on every event. The mounted query already owns that snapshot, so
        // only terminal snapshots enter the cross-route, session-local cache.
        service.detail_cache = Some(QueryCache::in_memory());
        service.log_cache = Some(QueryCache::in_memory());
        service
    }

    pub(crate) fn cached_section(&self, project: &str) -> Option<RunsSection> {
        self.section_cache?.get(&project)
    }

    pub(crate) fn cached_section_untracked(&self, project: &str) -> Option<RunsSection> {
        self.section_cache?.get_untracked(&project)
    }

    pub(crate) async fn load_section(&self, project: String) -> Result<RunsSection, ServerFnError> {
        let lifecycle_epoch = self
            .section_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let section = self.load_section.execute(project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.section_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.section_cache
        {
            cache.store(&key, &section);
        }
        Ok(section)
    }

    pub(crate) fn cached_detail(&self, project: &str, run_id: i64) -> Option<RunLogView> {
        self.detail_cache?.get(&(project, run_id))
    }

    pub(crate) fn cached_detail_untracked(&self, project: &str, run_id: i64) -> Option<RunLogView> {
        self.detail_cache?.get_untracked(&(project, run_id))
    }

    pub(crate) async fn load_detail(
        &self,
        project: String,
        run_id: i64,
    ) -> Result<RunLogView, ServerFnError> {
        let lifecycle_epoch = self
            .detail_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let detail = self.load_detail.execute((project, run_id)).await?;
        if !detail.active
            && lifecycle_epoch.is_some_and(|epoch| {
                self.detail_cache
                    .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
            })
            && let Some(cache) = self.detail_cache
        {
            cache.store(&(key, run_id), &detail);
        }
        Ok(detail)
    }

    pub(crate) fn cached_log(
        &self,
        project: &Option<String>,
        run_id: Option<i64>,
    ) -> Option<RunLogPage> {
        self.log_cache?.get(&(project, run_id))
    }

    pub(crate) fn cached_log_untracked(
        &self,
        project: &Option<String>,
        run_id: Option<i64>,
    ) -> Option<RunLogPage> {
        self.log_cache?.get_untracked(&(project, run_id))
    }

    pub(crate) async fn load_log(
        &self,
        project: Option<String>,
        run_id: Option<i64>,
    ) -> Result<RunLogPage, ServerFnError> {
        let lifecycle_epoch = self.log_cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let log = self.load_log.execute((project, run_id)).await?;
        if !log.run_log.active
            && lifecycle_epoch.is_some_and(|epoch| {
                self.log_cache
                    .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
            })
            && let Some(cache) = self.log_cache
        {
            cache.store(&(key, run_id), &log);
        }
        Ok(log)
    }

    pub(crate) async fn cancel_run(
        &self,
        project: String,
        run_id: i64,
    ) -> Result<(), ServerFnError> {
        self.cancel_run.execute((project, run_id)).await
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.section_cache {
            cache.clear();
        }
        if let Some(cache) = self.detail_cache {
            cache.clear();
        }
        if let Some(cache) = self.log_cache {
            cache.clear();
        }
    }
}

#[server(prefix = "/leptos")]
async fn load_runs_section(project: String) -> Result<RunsSection, ServerFnError> {
    let state = app_state::app_state();
    page_data::runs_section(
        &state.store,
        &state.sessions,
        &state.automation_controller,
        &project,
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_run_detail(project: String, run_id: i64) -> Result<RunLogView, ServerFnError> {
    let state = app_state::app_state();
    automation::read_run_log_with_active_session(&state.store, &state.sessions, &project, run_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_run_log_page(
    project: Option<String>,
    run_id: Option<i64>,
) -> Result<RunLogPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    match (project, run_id) {
        (Some(project), Some(run_id)) => page_data::run_log_page_data(
            &state.store,
            &state.sessions,
            &state.automation_controller,
            &project,
            run_id,
            codex_status,
        )
        .await
        .map_err(|err| ServerFnError::new(err.to_string())),
        _ => Err(ServerFnError::new("Missing run log route parameters")),
    }
}

#[server(prefix = "/leptos")]
async fn cancel_run(project: String, run_id: i64) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    let run = automation::get_run(&state.store, &project, run_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))?;
    if let Some(job_id) = run.knowledge_job_id {
        crate::backend::knowledge::jobs::action(
            &state.store,
            &state.sessions,
            &project,
            job_id,
            dispatch_types::knowledge::jobs::JobAction::Cancel,
        )
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
        return Ok(());
    }
    if run.status != AgentRunStatus::Running {
        return Err(ServerFnError::new(format!(
            "automation run {run_id} is not running"
        )));
    }
    if !state.sessions.cancel_run(&project, run_id) {
        return Err(ServerFnError::new(format!(
            "automation run {run_id} does not have an active session"
        )));
    }
    Ok(())
}
