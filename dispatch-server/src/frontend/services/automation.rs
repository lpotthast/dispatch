#[cfg(feature = "ssr")]
use crate::backend::{app_state, page_data};
use crate::frontend::{
    pages::{BoardRunSessionView, TriggersPage},
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct AutomationService {
    load_page: ServiceRequest<Option<String>, TriggersPage>,
    load_trigger_runs: ServiceRequest<(String, i64), Vec<BoardRunSessionView>>,
    page_cache: Option<LocalStorageCache<TriggersPage>>,
    trigger_runs_cache: Option<LocalStorageCache<Vec<BoardRunSessionView>>>,
}

impl AutomationService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<TriggersPage> + Send + Sync + 'static,
        load_trigger_runs: impl Fn((String, i64)) -> ServiceFuture<Vec<BoardRunSessionView>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            load_trigger_runs: ServiceRequest::new(load_trigger_runs),
            page_cache: None,
            trigger_runs_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_triggers_page(selected_project, api_base_url())),
            |(project, trigger_id)| Box::pin(load_trigger_run_sessions(project, trigger_id)),
        );
        service.page_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.automation.v1",
        ));
        service.trigger_runs_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.automation-trigger-runs.v1",
        ));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<TriggersPage> {
        self.page_cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<TriggersPage> {
        self.page_cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<TriggersPage, ServerFnError> {
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.page_cache {
            cache.store(&key, &page);
        }
        Ok(page)
    }

    pub(crate) fn cached_trigger_runs(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<BoardRunSessionView>> {
        self.trigger_runs_cache?.get(&(project, trigger_id))
    }

    pub(crate) fn cached_trigger_runs_untracked(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<BoardRunSessionView>> {
        self.trigger_runs_cache?
            .get_untracked(&(project, trigger_id))
    }

    pub(crate) async fn load_trigger_runs(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<Vec<BoardRunSessionView>, ServerFnError> {
        let key = project.clone();
        let runs = self
            .load_trigger_runs
            .execute((project, trigger_id))
            .await?;
        if let Some(cache) = self.trigger_runs_cache {
            cache.store(&(key, trigger_id), &runs);
        }
        Ok(runs)
    }
}

#[server(prefix = "/leptos")]
async fn load_triggers_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<TriggersPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::triggers_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
        api_base_url,
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_trigger_run_sessions(
    project: String,
    trigger_id: i64,
) -> Result<Vec<BoardRunSessionView>, ServerFnError> {
    let state = app_state::app_state();
    page_data::trigger_run_sessions(&state.store, &state.sessions, &project, trigger_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}
