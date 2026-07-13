#[cfg(feature = "ssr")]
use crate::backend::{
    app_state,
    comments::{self, AddComment},
    item_label_service, items, page_data, relationships,
};
use crate::frontend::{
    pages::ItemPage,
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
};
use crate::shared::view_models::{
    AddCommentRequest, CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest,
    UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
enum ItemMutation {
    AddComment {
        project: String,
        item_id: i64,
        request: AddCommentRequest,
    },
    AddLabel {
        project: String,
        item_id: i64,
        request: CreateWorkItemLabelRequest,
        expect_version: i64,
    },
    UpdateLabel {
        project: String,
        item_id: i64,
        label_id: i64,
        request: UpdateWorkItemLabelRequest,
    },
    DeleteLabel {
        project: String,
        item_id: i64,
        label_id: i64,
        expect_version: i64,
    },
    Move {
        project: String,
        item_id: i64,
        state: String,
        expect_version: i64,
    },
    AddRelationship {
        project: String,
        item_id: i64,
        request: CreateWorkItemRelationshipRequest,
    },
    UpdateRelationship {
        project: String,
        item_id: i64,
        relationship_id: i64,
        request: UpdateWorkItemRelationshipRequest,
    },
    DeleteRelationship {
        project: String,
        item_id: i64,
        relationship_id: i64,
    },
}

#[derive(Clone)]
pub(crate) struct ItemService {
    load_page: ServiceRequest<(Option<String>, Option<i64>), ItemPage>,
    mutate: ServiceRequest<ItemMutation, ()>,
    cache: Option<LocalStorageCache<ItemPage>>,
}

impl ItemService {
    fn new(
        load_page: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<ItemPage>
        + Send
        + Sync
        + 'static,
        mutate: impl Fn(ItemMutation) -> ServiceFuture<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            mutate: ServiceRequest::new(mutate),
            cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |(project, item_id)| Box::pin(load_item_page(project, item_id, api_base_url())),
            |mutation| Box::pin(mutate_item(mutation)),
        );
        service.cache = Some(LocalStorageCache::persistent("dispatch.query.items.v1"));
        service
    }

    pub(crate) fn cached_page(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemPage> {
        self.cache?.get(&("page", project, item_id))
    }

    pub(crate) fn cached_page_untracked(
        &self,
        project: &Option<String>,
        item_id: Option<i64>,
    ) -> Option<ItemPage> {
        self.cache?.get_untracked(&("page", project, item_id))
    }

    pub(crate) async fn load_page(
        &self,
        project: Option<String>,
        item_id: Option<i64>,
    ) -> Result<ItemPage, ServerFnError> {
        let key = project.clone();
        let page = self.load_page.execute((project, item_id)).await?;
        if let Some(cache) = self.cache {
            cache.store(&("page", key, item_id), &page);
        }
        Ok(page)
    }

    pub(crate) async fn add_comment(
        &self,
        project: String,
        item_id: i64,
        request: AddCommentRequest,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::AddComment {
                project,
                item_id,
                request,
            })
            .await
    }

    pub(crate) async fn add_label(
        &self,
        project: String,
        item_id: i64,
        request: CreateWorkItemLabelRequest,
        expect_version: i64,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::AddLabel {
                project,
                item_id,
                request,
                expect_version,
            })
            .await
    }

    pub(crate) async fn update_label(
        &self,
        project: String,
        item_id: i64,
        label_id: i64,
        request: UpdateWorkItemLabelRequest,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::UpdateLabel {
                project,
                item_id,
                label_id,
                request,
            })
            .await
    }

    pub(crate) async fn delete_label(
        &self,
        project: String,
        item_id: i64,
        label_id: i64,
        expect_version: i64,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::DeleteLabel {
                project,
                item_id,
                label_id,
                expect_version,
            })
            .await
    }

    pub(crate) async fn move_item(
        &self,
        project: String,
        item_id: i64,
        state: String,
        expect_version: i64,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::Move {
                project,
                item_id,
                state,
                expect_version,
            })
            .await
    }

    pub(crate) async fn add_relationship(
        &self,
        project: String,
        item_id: i64,
        request: CreateWorkItemRelationshipRequest,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::AddRelationship {
                project,
                item_id,
                request,
            })
            .await
    }

    pub(crate) async fn update_relationship(
        &self,
        project: String,
        item_id: i64,
        relationship_id: i64,
        request: UpdateWorkItemRelationshipRequest,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::UpdateRelationship {
                project,
                item_id,
                relationship_id,
                request,
            })
            .await
    }

    pub(crate) async fn delete_relationship(
        &self,
        project: String,
        item_id: i64,
        relationship_id: i64,
    ) -> Result<(), ServerFnError> {
        self.mutate
            .execute(ItemMutation::DeleteRelationship {
                project,
                item_id,
                relationship_id,
            })
            .await
    }
}

#[server(prefix = "/leptos")]
async fn load_item_page(
    project: Option<String>,
    item_id: Option<i64>,
    api_base_url: String,
) -> Result<ItemPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    match (project, item_id) {
        (Some(project), Some(item_id)) => page_data::item_page_data(
            &state.store,
            &state.automation_controller,
            &project,
            item_id,
            api_base_url,
            codex_status,
        )
        .await
        .map_err(|err| ServerFnError::new(err.to_string())),
        _ => Err(ServerFnError::new("Missing item route parameters")),
    }
}

#[server(prefix = "/leptos")]
async fn mutate_item(mutation: ItemMutation) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    let result = match mutation {
        ItemMutation::AddComment {
            project,
            item_id,
            request,
        } => comments::add_comment(
            &state.store,
            &project,
            item_id,
            AddComment {
                author_type: request.author_type,
                author_name: request.author_name.filter(|value| !value.trim().is_empty()),
                body: request.body,
            },
        )
        .await
        .map(|_| ()),
        ItemMutation::AddLabel {
            project,
            item_id,
            request,
            expect_version,
        } => item_label_service::add_label(
            &state.store,
            &project,
            item_id,
            request.key,
            request.value,
            Some(expect_version),
        )
        .await
        .map(|_| ()),
        ItemMutation::UpdateLabel {
            project,
            item_id,
            label_id,
            request,
        } => item_label_service::update_label(
            &state.store,
            &project,
            item_id,
            label_id,
            request.key,
            request.value,
            request.expect_version,
        )
        .await
        .map(|_| ()),
        ItemMutation::DeleteLabel {
            project,
            item_id,
            label_id,
            expect_version,
        } => item_label_service::delete_label(
            &state.store,
            &project,
            item_id,
            label_id,
            Some(expect_version),
        )
        .await
        .map(|_| ()),
        ItemMutation::Move {
            project,
            item_id,
            state: item_state,
            expect_version,
        } => items::move_item(
            &state.store,
            &project,
            item_id,
            item_state,
            Some(expect_version),
        )
        .await
        .map(|_| ()),
        ItemMutation::AddRelationship {
            project,
            item_id,
            request,
        } => relationships::create_relationship(
            &state.store,
            &project,
            item_id,
            request.target_work_item_id,
            request.kind,
        )
        .await
        .map(|_| ()),
        ItemMutation::UpdateRelationship {
            project,
            item_id,
            relationship_id,
            request,
        } => relationships::update_relationship_for_item(
            &state.store,
            &project,
            item_id,
            relationship_id,
            request.kind,
        )
        .await
        .map(|_| ()),
        ItemMutation::DeleteRelationship {
            project,
            item_id,
            relationship_id,
        } => relationships::delete_relationship_for_item(
            &state.store,
            &project,
            item_id,
            relationship_id,
        )
        .await
        .map(|_| ()),
    };
    result.map_err(|err| ServerFnError::new(err.to_string()))
}
