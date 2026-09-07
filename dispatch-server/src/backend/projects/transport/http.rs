use crate::backend::projects::controller::ProjectController;
use crate::backend::{
    app_state::AppState,
    http::{error_response, parse_optional_reasoning_effort},
    projects::{self, UpdateProjectSettings},
};
use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use dispatch_types::{
    AgentGitCommandPolicy, AgentGitHardResetPolicy, AgentToolName, RevertStrategy, WorkspaceMode,
    WorktreeCleanupPolicy,
};
use std::path::PathBuf;

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/projects", post(create_project))
        .route("/projects/{project}/update", post(update_project))
        .route("/projects/{project}/delete", post(delete_project))
        .route(
            "/projects/{project}/system-prompt",
            post(update_system_prompt),
        )
        .route(
            "/projects/{project}/system-prompt/events/clear",
            post(clear_system_prompt_history),
        )
        .route("/projects/{project}/settings", post(update_settings))
        .route(
            "/projects/{project}/settings/auto-commit",
            post(update_auto_commit),
        )
        .route(
            "/projects/{project}/settings/commit-policy",
            post(update_commit_policy),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

#[derive(serde::Deserialize)]
struct CreateProjectForm {
    name: String,
    display_name: Option<String>,
    path: String,
    default_agent_model: Option<String>,
    default_agent_reasoning_effort: Option<String>,
    system_prompt: Option<String>,
}

async fn create_project(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Form(form): Form<CreateProjectForm>,
) -> Response {
    controller.create_project(form).await
}

#[derive(serde::Deserialize)]
struct UpdateProjectForm {
    display_name: String,
    path: Option<String>,
}

async fn update_project(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
    Form(form): Form<UpdateProjectForm>,
) -> Response {
    controller.update_project(project, form).await
}

async fn delete_project(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
) -> Response {
    controller.delete_project(project).await
}

#[derive(serde::Deserialize)]
struct ProjectTextForm {
    body: String,
}

async fn update_system_prompt(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
    Form(form): Form<ProjectTextForm>,
) -> Response {
    controller.update_system_prompt(project, form).await
}

async fn clear_system_prompt_history(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
) -> Response {
    controller.clear_system_prompt_history(project).await
}

#[derive(serde::Deserialize)]
struct UpdateSettingsForm {
    knowledge_directory: Option<String>,
    workspace_mode: String,
    max_code_edit_agents: i64,
    max_read_only_agents: Option<i64>,
    create_pr: Option<String>,
    auto_commit: Option<String>,
    commit_standard: Option<String>,
    revert_strategy: Option<String>,
    stale_claim_minutes: i64,
    worktree_cleanup_policy: String,
    default_agent_tool: String,
    default_agent_model: Option<String>,
    default_agent_reasoning_effort: Option<String>,
    agent_sandbox_mode: Option<String>,
    agent_extra_writable_roots: Option<String>,
}

async fn update_settings(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
    Form(form): Form<UpdateSettingsForm>,
) -> Response {
    controller.update_settings(project, form).await
}

#[derive(serde::Deserialize)]
struct UpdateAutoCommitForm {
    enabled: bool,
}

async fn update_auto_commit(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
    Form(form): Form<UpdateAutoCommitForm>,
) -> Response {
    controller.update_auto_commit(project, form).await
}

#[derive(serde::Deserialize)]
struct UpdateCommitPolicyForm {
    max_read_only_agents: Option<i64>,
    auto_commit: Option<String>,
    commit_standard: Option<String>,
    revert_strategy: String,
    git_add: Option<String>,
    git_commit: Option<String>,
    git_push: Option<String>,
    git_reset: Option<String>,
    git_hard_reset: String,
}

async fn update_commit_policy(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
    Form(form): Form<UpdateCommitPolicyForm>,
) -> Response {
    controller.update_commit_policy(project, form).await
}

fn parse_optional_checkbox(value: Option<String>) -> Option<bool> {
    value.map(|value| {
        !matches!(
            value.trim().to_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    })
}

impl ProjectController {
    async fn create_project(self: std::sync::Arc<Self>, form: CreateProjectForm) -> Response {
        let display_name = form.display_name.filter(|value| !value.trim().is_empty());
        let path = PathBuf::from(form.path);
        let system_prompt = form.system_prompt.filter(|value| !value.trim().is_empty());
        let default_agent_reasoning_effort =
            match parse_optional_reasoning_effort(form.default_agent_reasoning_effort) {
                Ok(value) => value,
                Err(err) => return error_response(err).await,
            };
        match self
            .projects
            .create(projects::CreateProject {
                name: form.name.clone(),
                display_name,
                path,
                default_agent_model: form.default_agent_model,
                default_agent_reasoning_effort,
                system_prompt,
                memory: None,
            })
            .await
        {
            Ok(project) => {
                Redirect::to(&format!("/?project={}", urlencoding::encode(&project.name)))
                    .into_response()
            }
            Err(err) => error_response(err).await,
        }
    }
    async fn update_project(
        self: std::sync::Arc<Self>,
        project: String,
        form: UpdateProjectForm,
    ) -> Response {
        let display_name = Some(form.display_name);
        let path = form
            .path
            .filter(|value| !value.trim().is_empty())
            .map(|path| projects::ProjectPathUpdate::Set(PathBuf::from(path)));
        match self
            .projects
            .update(&project, projects::UpdateProject { display_name, path })
            .await
        {
            Ok(_) => Redirect::to(&format!(
                "/projects?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn delete_project(self: std::sync::Arc<Self>, project: String) -> Response {
        match self.project_deletion.delete_by_name(&project).await {
            Ok(()) => Redirect::to("/projects").into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_system_prompt(
        self: std::sync::Arc<Self>,
        project: String,
        form: ProjectTextForm,
    ) -> Response {
        match self
            .projects
            .update_system_prompt(&project, form.body)
            .await
        {
            Ok(_) => Redirect::to(&format!(
                "/project?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn clear_system_prompt_history(self: std::sync::Arc<Self>, project: String) -> Response {
        match self.projects.clear_system_prompt_history(&project).await {
            Ok(_) => Redirect::to(&format!(
                "/project?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_settings(
        self: std::sync::Arc<Self>,
        project: String,
        form: UpdateSettingsForm,
    ) -> Response {
        let workspace_mode = match form.workspace_mode.parse::<WorkspaceMode>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let worktree_cleanup_policy = match form
            .worktree_cleanup_policy
            .parse::<WorktreeCleanupPolicy>()
        {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let revert_strategy = match form.revert_strategy {
            Some(value) => match value.parse::<RevertStrategy>() {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };
        let default_agent_tool = match form.default_agent_tool.parse::<AgentToolName>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let agent_sandbox_mode = match form.agent_sandbox_mode {
            Some(value) => match value.parse() {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };
        let default_agent_reasoning_effort =
            match parse_optional_reasoning_effort(form.default_agent_reasoning_effort) {
                Ok(value) => value,
                Err(err) => return error_response(err).await,
            };
        let agent_extra_writable_roots = match form.agent_extra_writable_roots {
            Some(value) => match projects::parse_agent_extra_writable_roots_text(&value) {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };

        match self
            .projects
            .update_settings(
                &project,
                UpdateProjectSettings {
                    knowledge_directory: form.knowledge_directory,
                    workspace_mode: Some(workspace_mode),
                    max_code_edit_agents: Some(form.max_code_edit_agents),
                    max_read_only_agents: form.max_read_only_agents,
                    create_pr: Some(form.create_pr.is_some()),
                    auto_commit: parse_optional_checkbox(form.auto_commit),
                    commit_standard: form.commit_standard,
                    revert_strategy,
                    stale_claim_minutes: Some(form.stale_claim_minutes),
                    worktree_cleanup_policy: Some(worktree_cleanup_policy),
                    default_agent_tool: Some(default_agent_tool),
                    default_agent_model: Some(
                        form.default_agent_model
                            .filter(|value| !value.trim().is_empty()),
                    ),
                    default_agent_reasoning_effort: Some(default_agent_reasoning_effort),
                    agent_sandbox_mode,
                    agent_extra_writable_roots,
                    agent_git_command_policy: None,
                },
            )
            .await
        {
            Ok(_) => Redirect::to(&format!(
                "/project?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_auto_commit(
        self: std::sync::Arc<Self>,
        project: String,
        form: UpdateAutoCommitForm,
    ) -> Response {
        match self
            .projects
            .update_settings(
                &project,
                UpdateProjectSettings {
                    auto_commit: Some(form.enabled),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_commit_policy(
        self: std::sync::Arc<Self>,
        project: String,
        form: UpdateCommitPolicyForm,
    ) -> Response {
        let revert_strategy = match form.revert_strategy.parse::<RevertStrategy>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let git_hard_reset = match form.git_hard_reset.parse::<AgentGitHardResetPolicy>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let agent_git_command_policy = AgentGitCommandPolicy {
            add: form.git_add.is_some(),
            commit: form.git_commit.is_some(),
            push: form.git_push.is_some(),
            reset: form.git_reset.is_some(),
            hard_reset: git_hard_reset,
        };
        match self
            .projects
            .update_settings(
                &project,
                UpdateProjectSettings {
                    max_read_only_agents: form.max_read_only_agents,
                    auto_commit: Some(form.auto_commit.is_some()),
                    commit_standard: form.commit_standard,
                    revert_strategy: Some(revert_strategy),
                    agent_git_command_policy: Some(agent_git_command_policy),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(_) => Redirect::to(&format!(
                "/automation?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
            Err(err) => error_response(err).await,
        }
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.project_controller.clone());
    next.run(request).await
}
