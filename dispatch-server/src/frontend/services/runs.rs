#[cfg(feature = "ssr")]
use crate::backend::{app_state, automation, page_data};
use crate::frontend::{
    pages::{RunLogPage, RunsPage, RunsSection},
    services::{
        cache::LocalStorageCache,
        request::{ServiceFuture, ServiceRequest},
    },
};
#[cfg(feature = "ssr")]
use dispatch_types::AgentRunStatus;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct RunService {
    load_page: ServiceRequest<Option<String>, RunsPage>,
    load_section: ServiceRequest<String, RunsSection>,
    load_log: ServiceRequest<(Option<String>, Option<i64>), RunLogPage>,
    cancel_run: ServiceRequest<(String, i64), ()>,
    page_cache: Option<LocalStorageCache<RunsPage>>,
    section_cache: Option<LocalStorageCache<RunsSection>>,
    log_cache: Option<LocalStorageCache<RunLogPage>>,
}

impl RunService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<RunsPage> + Send + Sync + 'static,
        load_section: impl Fn(String) -> ServiceFuture<RunsSection> + Send + Sync + 'static,
        load_log: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<RunLogPage>
        + Send
        + Sync
        + 'static,
        cancel_run: impl Fn((String, i64)) -> ServiceFuture<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            load_section: ServiceRequest::new(load_section),
            load_log: ServiceRequest::new(load_log),
            cancel_run: ServiceRequest::new(cancel_run),
            page_cache: None,
            section_cache: None,
            log_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_runs_page(selected_project)),
            |project| Box::pin(load_runs_section(project)),
            |(project, run_id)| Box::pin(load_run_log_page(project, run_id)),
            |(project, run_id)| Box::pin(cancel_run(project, run_id)),
        );
        service.page_cache = Some(LocalStorageCache::persistent("dispatch.query.runs.v1"));
        service.section_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.runs-section.v1",
        ));
        service.log_cache = Some(LocalStorageCache::persistent("dispatch.query.run-logs.v1"));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<RunsPage> {
        self.page_cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<RunsPage> {
        self.page_cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<RunsPage, ServerFnError> {
        let lifecycle_epoch = self.page_cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.page_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) {
            if let (Some(cache), Some((project, section))) =
                (self.section_cache, runs_section_from_page(&page))
            {
                cache.store(&project, &section);
            }
            if let Some(cache) = self.page_cache {
                cache.store(&key, &page);
            }
        }
        Ok(page)
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
        if lifecycle_epoch.is_some_and(|epoch| {
            self.log_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.log_cache
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
        if let Some(cache) = self.page_cache {
            cache.clear();
        }
        if let Some(cache) = self.section_cache {
            cache.clear();
        }
        if let Some(cache) = self.log_cache {
            cache.clear();
        }
    }
}

fn runs_section_from_page(page: &RunsPage) -> Option<(String, RunsSection)> {
    Some((
        page.selected_project.clone()?,
        RunsSection {
            automation_status: page.automation_status.clone()?,
            automation_running: page.automation_running,
            run_sessions: page.run_sessions.clone(),
        },
    ))
}

#[server(prefix = "/leptos")]
async fn load_runs_page(selected_project: Option<String>) -> Result<RunsPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::runs_page_data(
        &state.store,
        &state.sessions,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
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
    if run.status != AgentRunStatus::Running {
        return Err(ServerFnError::new(format!(
            "automation run {run_id} is not running"
        )));
    }
    if !state.sessions.cancel_run(&project, run_id).await {
        return Err(ServerFnError::new(format!(
            "automation run {run_id} does not have an active session"
        )));
    }
    Ok(())
}
