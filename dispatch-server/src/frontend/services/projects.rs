#[cfg(feature = "ssr")]
use crate::backend::{
    app_state, page_data,
    projects::{self, UpdateProjectSettings},
};
use crate::{
    frontend::{
        pages::ProjectsPage,
        services::{
            cache::LocalStorageCache,
            origin::api_base_url,
            request::{ServiceFuture, ServiceRequest},
        },
    },
    shared::view_models::ProjectView,
};
use codee::string::JsonSerdeCodec;
use leptos::prelude::*;
use leptos_use::storage::{UseStorageOptions, use_local_storage_with_options};

const PROJECT_CACHE_STORAGE_KEY: &str = "dispatch.projects.v1";

#[derive(Clone)]
pub(crate) struct ProjectService {
    load_page: ServiceRequest<Option<String>, ProjectsPage>,
    update_auto_commit: ServiceRequest<(String, bool), ()>,
    crudkit_api_base_url: String,
    cache: Option<LocalStorageCache<ProjectsPage>>,
}

impl ProjectService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<ProjectsPage> + Send + Sync + 'static,
        update_auto_commit: impl Fn((String, bool)) -> ServiceFuture<()> + Send + Sync + 'static,
        crudkit_api_base_url: String,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            update_auto_commit: ServiceRequest::new(update_auto_commit),
            crudkit_api_base_url,
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let crudkit_api_base_url = api_base_url();
        let page_api_base_url = crudkit_api_base_url.clone();
        let mut service = Self::new(
            move |selected_project| {
                Box::pin(load_projects_page(
                    selected_project,
                    page_api_base_url.clone(),
                ))
            },
            |(project, enabled)| Box::pin(update_auto_commit(project, enabled)),
            crudkit_api_base_url,
        );
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.projects.v1"));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<ProjectsPage> {
        self.cache?.get(&("page", selected_project))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<ProjectsPage> {
        self.cache?.get_untracked(&("page", selected_project))
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ProjectsPage, ServerFnError> {
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.cache {
            cache.store(&("page", key), &page);
        }
        Ok(page)
    }

    pub(crate) async fn update_auto_commit(
        &self,
        project: String,
        enabled: bool,
    ) -> Result<(), ServerFnError> {
        self.update_auto_commit.execute((project, enabled)).await
    }

    pub(crate) fn crudkit_api_base_url(&self) -> &str {
        &self.crudkit_api_base_url
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ProjectCache {
    projects: Signal<Vec<ProjectView>>,
    set_projects: WriteSignal<Vec<ProjectView>>,
}

#[server(prefix = "/leptos")]
async fn load_projects_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<ProjectsPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::projects_page_data(
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
async fn update_auto_commit(project: String, enabled: bool) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    projects::update_settings(
        &state.store,
        &project,
        UpdateProjectSettings {
            auto_commit: Some(enabled),
            ..Default::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn provide_project_cache() {
    let (projects, set_projects, _) =
        use_local_storage_with_options::<Vec<ProjectView>, JsonSerdeCodec>(
            PROJECT_CACHE_STORAGE_KEY,
            UseStorageOptions::default().delay_during_hydration(true),
        );
    provide_context(ProjectCache {
        projects,
        set_projects,
    });
}

pub(crate) fn project_cache() -> ProjectCache {
    expect_context::<ProjectCache>()
}

impl ProjectCache {
    pub(crate) fn track<T>(
        self,
        value: ReadSignal<Option<T>>,
        projects: impl for<'a> Fn(&'a T) -> &'a [ProjectView] + Copy + 'static,
    ) where
        T: Clone + Send + Sync + 'static,
    {
        Effect::new(move |_| {
            if let Some(value) = value.get() {
                self.store(projects(&value));
            }
        });
    }

    pub(crate) fn projects(self) -> Signal<Vec<ProjectView>> {
        self.projects
    }

    fn store(self, projects: &[ProjectView]) {
        if self.projects.with_untracked(|cached| cached != projects) {
            self.set_projects.set(projects.to_vec());
        }
    }
}
