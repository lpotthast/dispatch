use crate::backend::items::controller::ItemController;
use crate::backend::{api::json_result, app_state::AppState, items::CreateWorkItem};
use async_stream::stream;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::HeaderMap,
    response::{
        Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use dispatch_types::{
    CreateWorkItemRequest, DEFAULT_STATE_LABEL, UpdateWorkItemRequest, WorkItemSearchRequest,
};
use futures_core::Stream;
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/items",
            get(list_items).post(create_item),
        )
        .route("/api/projects/{project}/items/search", post(search_items))
        .route(
            "/api/projects/{project}/items/{item_id}",
            get(get_item).patch(update_item),
        )
        .route("/api/projects/{project}/events", get(project_events))
        .route(
            "/api/projects/{project}/items/{item_id}/events",
            get(item_events),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
#[derive(Debug, Deserialize)]
struct EventsQuery {
    since: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListItemsQuery {
    state: Option<String>,
}

async fn list_items(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path(project): Path<String>,
    Query(query): Query<ListItemsQuery>,
) -> Response {
    controller.list_items(project, query).await
}

async fn search_items(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<WorkItemSearchRequest>,
) -> Response {
    controller.search_items(project, headers, request).await
}

pub(crate) async fn create_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemRequest>,
) -> Response {
    controller.create_item(project, headers, request).await
}

async fn get_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    controller.get_item((project, item_id), headers).await
}

pub(crate) async fn update_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRequest>,
) -> Response {
    controller
        .update_item((project, item_id), headers, request)
        .await
}

async fn project_events(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path(project): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    controller.project_events(project, query).await
}

async fn item_events(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path((project, item_id)): Path<(String, i64)>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    controller.item_events((project, item_id), query).await
}

fn event_stream(
    item_service: std::sync::Arc<crate::backend::items::service::ItemService>,
    project: String,
    item_id: Option<i64>,
    since: Option<i64>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let events = stream! {
        let mut last_id = since;
        loop {
            match item_service.events(&project, item_id, last_id).await {
                Ok(new_events) => {
                    for event in new_events {
                        last_id = Some(event.id);
                        let response = Event::default()
                            .id(event.id.to_string())
                            .event(event.event_type.as_storage())
                            .json_data(&event)
                            .unwrap_or_else(|err| {
                                Event::default()
                                    .event("error")
                                    .data(format!("failed to serialize event: {err}"))
                            });
                        yield Ok(response);
                    }
                }
                Err(err) => {
                    yield Ok(Event::default().event("error").data(err.to_string()));
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };

    Sse::new(events).keep_alive(KeepAlive::default())
}

impl ItemController {
    async fn list_items(
        self: std::sync::Arc<Self>,
        project: String,
        query: ListItemsQuery,
    ) -> Response {
        json_result(self.items.list(&project, query.state).await)
    }
    async fn search_items(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: WorkItemSearchRequest,
    ) -> Response {
        let result = async {
            crate::backend::attribution::transport::from_headers(
                &self.attribution,
                &project,
                &headers,
            )
            .await?;
            self.items.search(&project, request).await
        }
        .await;
        json_result(result)
    }
    async fn create_item(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: CreateWorkItemRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.item_creation
                .create(
                    crate::backend::projects::ProjectReference::Name(&project),
                    CreateWorkItem {
                        title: request.title,
                        description: request.description,
                        state: request
                            .state
                            .unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned()),
                        agent_model_override: request.agent_model_override,
                        agent_reasoning_effort_override: request.agent_reasoning_effort_override,
                        initial_labels: request.initial_labels,
                    },
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn get_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            crate::backend::attribution::transport::from_headers(
                &self.attribution,
                &project,
                &headers,
            )
            .await?;
            self.items.get(&project, item_id).await
        }
        .await;
        json_result(result)
    }
    async fn update_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: UpdateWorkItemRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.items
                .update(
                    crate::backend::projects::ProjectReference::Name(&project),
                    item_id,
                    UpdateWorkItemRequest {
                        title: request.title,
                        description: request.description,
                        state: request.state,
                        agent_model_override: request.agent_model_override,
                        agent_reasoning_effort_override: request.agent_reasoning_effort_override,
                        expect_version: request.expect_version,
                    },
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn project_events(
        self: std::sync::Arc<Self>,
        project: String,
        query: EventsQuery,
    ) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
        event_stream(self.items.clone(), project, None, query.since)
    }
    async fn item_events(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        query: EventsQuery,
    ) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
        event_stream(self.items.clone(), project, Some(item_id), query.since)
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.item_controller.clone());
    next.run(request).await
}
