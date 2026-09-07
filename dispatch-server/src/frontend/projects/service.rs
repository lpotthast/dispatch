use super::types::ProjectRequests;
#[cfg(feature = "ssr")]
use crate::backend::{
    app_state, execution::workspaces::model::WorkspaceOpenTarget, projects::UpdateProjectSettings,
};
use crate::{
    frontend::http::{origin::api_base_url, request::ServiceRequest},
    shared::view_models::HistoryClearResult,
};
use dispatch_types::CommitPolicyUpdate;
use dispatch_types::PickFolderResponse;
use dispatch_types::{ProjectPage, ProjectsPage, WorkspaceBarData};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ProjectService {
    http: crate::frontend::http::HttpService,
    load_page: ServiceRequest<(), ProjectsPage>,
    load_project_page: ServiceRequest<Option<String>, ProjectPage>,
    load_workspace_bar: ServiceRequest<Option<String>, WorkspaceBarData>,
    #[cfg(not(feature = "ssr"))]
    current_project_id: ServiceRequest<String, Option<i64>>,
    update_auto_commit: ServiceRequest<(String, bool), ()>,
    update_system_prompt: ServiceRequest<(String, String), ()>,
    clear_system_prompt_history: ServiceRequest<String, HistoryClearResult>,
    update_commit_policy: ServiceRequest<(String, CommitPolicyUpdate), ()>,
    open_workspace: ServiceRequest<(String, String), ()>,
    cleanup_worktrees: ServiceRequest<String, ()>,
    crudkit_api_base_url: String,
}

impl ProjectService {
    fn new(
        http: crate::frontend::http::HttpService,
        requests: ProjectRequests,
        crudkit_api_base_url: String,
    ) -> Self {
        Self {
            http,
            load_page: requests.load_page,
            load_project_page: requests.load_project_page,
            load_workspace_bar: requests.load_workspace_bar,
            #[cfg(not(feature = "ssr"))]
            current_project_id: requests.current_project_id,
            update_auto_commit: requests.update_auto_commit,
            update_system_prompt: requests.update_system_prompt,
            clear_system_prompt_history: requests.clear_system_prompt_history,
            update_commit_policy: requests.update_commit_policy,
            open_workspace: requests.open_workspace,
            cleanup_worktrees: requests.cleanup_worktrees,
            crudkit_api_base_url,
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        let crudkit_api_base_url = api_base_url();
        let project_page_api_base_url = crudkit_api_base_url.clone();
        Self::new(
            http.clone(),
            ProjectRequests {
                load_page: http.request(|()| Box::pin(load_projects_page())),
                load_project_page: http.request(move |selected_project| {
                    Box::pin(load_project_page(
                        selected_project,
                        project_page_api_base_url.clone(),
                    ))
                }),
                load_workspace_bar: http
                    .request(|selected_project| Box::pin(load_workspace_bar(selected_project))),
                #[cfg(not(feature = "ssr"))]
                current_project_id: http.request(|project| Box::pin(current_project_id(project))),
                update_auto_commit: http
                    .request(|(project, enabled)| Box::pin(update_auto_commit(project, enabled))),
                update_system_prompt: http
                    .request(|(project, body)| Box::pin(update_system_prompt(project, body))),
                clear_system_prompt_history: http
                    .request(|project| Box::pin(clear_system_prompt_history(project))),
                update_commit_policy: http
                    .request(|(project, update)| Box::pin(update_commit_policy(project, update))),
                open_workspace: http
                    .request(|(project, target)| Box::pin(open_workspace(project, target))),
                cleanup_worktrees: http.request(|project| Box::pin(cleanup_worktrees(project))),
            },
            crudkit_api_base_url,
        )
    }

    pub(crate) async fn pick_folder(&self) -> Result<Option<String>, ServerFnError> {
        let response: PickFolderResponse = self.http.post_empty("/system/pick-folder").await?;
        Ok(response
            .path
            .map(|path| path.trim().to_owned())
            .filter(|path| !path.is_empty()))
    }

    pub(crate) async fn load_page(&self) -> Result<ProjectsPage, ServerFnError> {
        self.load_page.execute(()).await
    }

    pub(crate) async fn load_project_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ProjectPage, ServerFnError> {
        self.load_project_page.execute(selected_project).await
    }

    pub(crate) async fn load_workspace_bar(
        &self,
        selected_project: Option<String>,
    ) -> Result<WorkspaceBarData, ServerFnError> {
        self.load_workspace_bar.execute(selected_project).await
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) async fn current_project_id(
        &self,
        project: String,
    ) -> Result<Option<i64>, ServerFnError> {
        self.current_project_id.execute(project).await
    }

    pub(crate) async fn update_auto_commit(
        &self,
        project: String,
        enabled: bool,
    ) -> Result<(), ServerFnError> {
        self.update_auto_commit.execute((project, enabled)).await
    }

    pub(crate) async fn update_system_prompt(
        &self,
        project: String,
        body: String,
    ) -> Result<(), ServerFnError> {
        self.update_system_prompt.execute((project, body)).await
    }

    pub(crate) async fn clear_system_prompt_history(
        &self,
        project: String,
    ) -> Result<HistoryClearResult, ServerFnError> {
        self.clear_system_prompt_history.execute(project).await
    }

    pub(crate) async fn update_commit_policy(
        &self,
        project: String,
        update: CommitPolicyUpdate,
    ) -> Result<(), ServerFnError> {
        self.update_commit_policy.execute((project, update)).await
    }

    pub(crate) async fn open_workspace(
        &self,
        project: String,
        target: String,
    ) -> Result<(), ServerFnError> {
        self.open_workspace.execute((project, target)).await
    }

    pub(crate) async fn cleanup_worktrees(&self, project: String) -> Result<(), ServerFnError> {
        self.cleanup_worktrees.execute(project).await
    }

    pub(crate) fn crudkit_api_base_url(&self) -> &str {
        &self.crudkit_api_base_url
    }
}

#[server(prefix = "/leptos")]
async fn load_projects_page() -> Result<ProjectsPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .projects_page()
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_project_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<ProjectPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .project_page(selected_project.as_deref(), api_base_url)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_workspace_bar(
    selected_project: Option<String>,
) -> Result<WorkspaceBarData, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .workspace_bar(selected_project.as_deref())
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn current_project_id(project: String) -> Result<Option<i64>, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .projects
        .find_id(&project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_auto_commit(project: String, enabled: bool) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .projects
        .update_settings(
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

#[server(prefix = "/leptos")]
async fn update_system_prompt(project: String, body: String) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .projects
        .update_system_prompt(&project, body)
        .await
        .map(|_| ())
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn clear_system_prompt_history(project: String) -> Result<HistoryClearResult, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .projects
        .clear_system_prompt_history(&project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn update_commit_policy(
    project: String,
    update: CommitPolicyUpdate,
) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .projects
        .update_settings(
            &project,
            UpdateProjectSettings {
                max_read_only_agents: Some(update.max_read_only_agents),
                auto_commit: Some(update.auto_commit),
                commit_standard: Some(update.commit_standard),
                revert_strategy: Some(update.revert_strategy),
                agent_git_command_policy: Some(update.agent_git_command_policy),
                ..Default::default()
            },
        )
        .await
        .map(|_| ())
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn open_workspace(project: String, target: String) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    let target =
        WorkspaceOpenTarget::parse(&target).map_err(|err| ServerFnError::new(err.to_string()))?;
    state
        .workspaces
        .open_project(&project, target)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn cleanup_worktrees(project: String) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .launch
        .cleanup_worktrees(&project, None)
        .await
        .map(|_| ())
        .map_err(|error| ServerFnError::new(error.to_string()))
}

pub(crate) fn project_service() -> ProjectService {
    leptos::prelude::expect_context()
}

#[cfg(all(test, feature = "ssr"))]
mod backend_adapter_tests {
    use super::*;
    use crate::backend::{application::Application, projects::CreateProject, storage::Store};
    use assertr::prelude::*;
    use leptos::reactive::computed::ScopedFuture;
    use tempfile::TempDir;

    #[tokio::test]
    async fn project_server_functions_use_request_services_for_prompt_history_and_settings() {
        let temp = TempDir::new().unwrap();
        let mut apps = vec![];
        let mut owners = vec![];
        for index in 0..2 {
            let store = Store::open_with_max_connections(
                temp.path().join(format!("app-{index}.sqlite3")),
                1,
            )
            .await
            .unwrap();
            let app = Application::from_store(store, format!("http://127.0.0.1:{}", 4400 + index));
            app.state
                .projects
                .create(CreateProject {
                    name: "demo".into(),
                    display_name: None,
                    path: temp.path().to_owned(),
                    default_agent_model: None,
                    default_agent_reasoning_effort: None,
                    system_prompt: Some("Initial prompt".into()),
                    memory: None,
                })
                .await
                .unwrap();
            let owner = Owner::new();
            owner.with(|| provide_context(app.state.clone()));
            owners.push(owner);
            apps.push(app);
        }
        let mut events = apps[0].state.events.subscribe();
        owners[0]
            .with(|| ScopedFuture::new(update_system_prompt("demo".into(), "New prompt".into())))
            .await
            .unwrap();
        let history = apps[0]
            .state
            .projects
            .system_prompt_events("demo")
            .await
            .unwrap();
        assert_that!(&history.len()).is_equal_to(2);
        assert_that!(&history[0].system_prompt).is_equal_to("New prompt");
        assert_that!(&history[0].actor_type.as_deref()).is_equal_to(Some("user"));
        assert_that!(&matches!(
            events.try_recv().unwrap(),
            dispatch_types::UiEvent::SystemPromptChanged { .. }
        ))
        .is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(
            &apps[1]
                .state
                .projects
                .get("demo")
                .await
                .unwrap()
                .system_prompt
        )
        .is_equal_to("Initial prompt");
        owners[1]
            .with(|| ScopedFuture::new(update_auto_commit("demo".into(), false)))
            .await
            .unwrap();
        assert_that!(
            &apps[0]
                .state
                .projects
                .settings("demo")
                .await
                .unwrap()
                .auto_commit
        )
        .is_true();
        assert_that!(
            &apps[1]
                .state
                .projects
                .settings("demo")
                .await
                .unwrap()
                .auto_commit
        )
        .is_false();
        let cleared = owners[0]
            .with(|| ScopedFuture::new(clear_system_prompt_history("demo".into())))
            .await
            .unwrap();
        assert_that!(&cleared.deleted_events).is_equal_to(2);
        assert_that!(
            &apps[0]
                .state
                .projects
                .get("demo")
                .await
                .unwrap()
                .system_prompt
        )
        .is_equal_to("New prompt");
        assert_that!(
            &apps[1]
                .state
                .projects
                .system_prompt_events("demo")
                .await
                .unwrap()
                .len()
        )
        .is_equal_to(1);
    }
    #[tokio::test]
    async fn initial_ssr_preserves_the_shell_and_http_server_functions_use_each_context() {
        use leptos::server_fn::ServerFn;
        let temp = TempDir::new().unwrap();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        let mut apps = Vec::new();
        let mut servers = Vec::new();
        for name in ["ssr-first", "ssr-second"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let url = format!("http://{address}");
            let store =
                Store::open_with_max_connections(temp.path().join(format!("{name}.sqlite3")), 1)
                    .await
                    .unwrap();
            let app = Application::from_store(store, url.clone());
            app.state
                .projects
                .create(CreateProject {
                    name: name.into(),
                    display_name: None,
                    path: temp.path().to_owned(),
                    default_agent_model: None,
                    default_agent_reasoning_effort: None,
                    system_prompt: None,
                    memory: None,
                })
                .await
                .unwrap();
            let options = leptos::prelude::LeptosOptions::builder()
                .output_name("dispatch-server")
                .site_addr(address)
                .build();
            let router =
                crate::backend::http::router(app.state.clone(), app.contexts.clone(), options);
            servers.push(tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap()
            }));
            apps.push((name, url, app));
        }
        for (name, url, app) in &apps {
            let response = client.get(format!("{url}/projects")).send().await.unwrap();
            assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
            let html = response.text().await.unwrap();
            assert_that!(&html.contains("Projects")).is_true();
            assert_that!(&html.contains("dispatch-server")).is_true();
            let response = client
                .post(format!("{url}{}", LoadProjectsPage::PATH))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .body("")
                .send()
                .await
                .unwrap();
            assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
            let page = response.json::<ProjectsPage>().await.unwrap();
            assert_that!(&page.projects.len()).is_equal_to(1);
            assert_that!(&page.projects[0].name).is_equal_to(*name);
            let mut events = app.state.events.subscribe();
            let response = client
                .post(format!("{url}{}", UpdateSystemPrompt::PATH))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .body(format!("project={name}&body=HTTP%20prompt"))
                .send()
                .await
                .unwrap();
            assert_that!(&response.status()).is_equal_to(reqwest::StatusCode::OK);
            assert_that!(&app.state.projects.get(name).await.unwrap().system_prompt)
                .is_equal_to("HTTP prompt");
            assert_that!(&events.try_recv().is_ok()).is_true();
        }
        for server in servers {
            server.abort();
        }
    }
}
