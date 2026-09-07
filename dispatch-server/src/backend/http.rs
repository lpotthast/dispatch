use axum::{
    Extension, Router,
    response::{IntoResponse, Redirect, Response},
};
use leptos::prelude::LeptosOptions;
use leptos_axum::{LeptosRoutes, generate_route_list};
use rootcause::{Result, prelude::*};

use crate::{
    backend::{api, app_state::AppState, crudkit_resources, projects},
    frontend,
    shared::view_models::AgentReasoningEffort,
};

pub(crate) fn router(
    state: AppState,
    contexts: crudkit_resources::CrudContexts,
    leptos_options: LeptosOptions,
) -> Router<()> {
    let routes = generate_route_list(frontend::App);

    let mut crud_router = Router::new();
    crud_router = crud_router.merge(projects::transport::crud::routes());
    crud_router = crud_router.merge(crate::backend::items::transport::crud::routes());
    crud_router = crud_router.merge(crate::backend::comments::transport::crud::routes());
    crud_router = crud_router.merge(crate::backend::execution::tools::transport::routes());
    crud_router = crud_router.merge(super::runs::transport::crud::routes());
    crud_router = crud_router.merge(super::automation::rules::transport::crud::routes());
    crud_router = crud_router.merge(super::automation::personalities::transport::crud::routes());
    crud_router = crud_router.merge(crate::backend::board::lanes::transport::routes());
    crud_router = crud_router.merge(crate::backend::items::states::transport::routes());
    crud_router = crud_router.merge(crate::backend::items::labels::catalog::transport::routes());

    let leptos_shell = {
        let leptos_options = leptos_options.clone();
        move || frontend::shell(leptos_options.clone())
    };
    let request_state = state.clone();
    let render_pool = tokio_util::task::LocalPoolHandle::new(
        std::thread::available_parallelism().map_or(1, usize::from),
    );
    let ui = Router::<LeptosOptions>::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || leptos::prelude::provide_context(request_state.clone()),
            leptos_shell,
        )
        .fallback(leptos_axum::file_and_error_handler(frontend::shell))
        .layer(axum::middleware::from_fn_with_state(
            render_pool,
            frontend::app::ssr::render_on_worker,
        ));
    Router::<LeptosOptions>::new()
        .merge(super::execution::workspaces::transport::routes())
        .merge(projects::transport::http::routes())
        .merge(crate::backend::items::transport::http::routes())
        .merge(super::automation::transport::routes())
        .merge(super::runs::transport::forms::routes())
        .merge(super::automation::rules::transport::forms::routes())
        .merge(super::execution::codex::transport::routes())
        .merge(api::router::<LeptosOptions>())
        .merge(crud_router.with_state(()))
        .merge(ui)
        .layer(Extension(state))
        .layer(Extension(contexts.project))
        .layer(Extension(contexts.work_item))
        .layer(Extension(contexts.comment))
        .layer(Extension(contexts.agent_tool))
        .layer(Extension(contexts.agent_run))
        .layer(Extension(contexts.automation_trigger))
        .layer(Extension(contexts.personality))
        .layer(Extension(contexts.swim_lane))
        .layer(Extension(contexts.work_item_state))
        .layer(Extension(contexts.label_key))
        .with_state(leptos_options)
}

pub(crate) async fn error_response(err: impl Into<Report>) -> Response {
    let err = err.into();
    Redirect::to(&format!(
        "/error?message={}",
        urlencoding::encode(&err.to_string())
    ))
    .into_response()
}

pub(crate) fn safe_return_to(return_to: Option<String>, fallback: String) -> String {
    return_to
        .filter(|target| target.starts_with('/') && !target.starts_with("//"))
        .unwrap_or(fallback)
}

pub(crate) fn parse_optional_reasoning_effort(
    value: Option<String>,
) -> Result<Option<AgentReasoningEffort>> {
    match value.filter(|value| !value.trim().is_empty()) {
        Some(value) => Ok(Some(value.parse::<AgentReasoningEffort>()?)),
        None => Ok(None),
    }
}
