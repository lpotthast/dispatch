#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::{
    origin::api_base_url,
    request::{ServiceFuture, ServiceRequest},
};
use crate::shared::view_models::{
    AddCommentRequest, CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest,
    UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest,
};
use dispatch_types::ItemMutation;
use dispatch_types::ItemPage;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ItemService {
    load_page: ServiceRequest<(Option<String>, Option<i64>), ItemPage>,
    mutate: ServiceRequest<ItemMutation, ()>,
}

impl ItemService {
    fn new(
        http: crate::frontend::http::HttpService,
        load_page: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<ItemPage>
        + Send
        + Sync
        + 'static,
        mutate: impl Fn(ItemMutation) -> ServiceFuture<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: http.request(load_page),
            mutate: http.request(mutate),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(
            http.clone(),
            |(project, item_id)| Box::pin(load_item_page(project, item_id, api_base_url())),
            |mutation| Box::pin(mutate_item(mutation)),
        )
    }

    pub(crate) async fn load_page(
        &self,
        project: Option<String>,
        item_id: Option<i64>,
    ) -> Result<ItemPage, ServerFnError> {
        self.load_page.execute((project, item_id)).await
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
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    match (project, item_id) {
        (Some(project), Some(item_id)) => state
            .operator_queries
            .item_page(&project, item_id, api_base_url)
            .await
            .map_err(|err| ServerFnError::new(err.to_string())),
        _ => Err(ServerFnError::new("Missing item route parameters")),
    }
}

#[server(prefix = "/leptos")]
async fn mutate_item(mutation: ItemMutation) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    let result = match mutation {
        ItemMutation::AddComment {
            project,
            item_id,
            request,
        } => state
            .comments
            .add(
                crate::backend::comments::CommentTarget::ProjectItem {
                    project: &project,
                    item_id,
                },
                AddCommentRequest {
                    author_type: request.author_type,
                    author_name: request.author_name.filter(|value| !value.trim().is_empty()),
                    body: request.body,
                },
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::AddLabel {
            project,
            item_id,
            request,
            expect_version,
        } => state
            .labels
            .add(
                &project,
                item_id,
                crate::shared::view_models::CreateWorkItemLabelRequest {
                    key: request.key,
                    value: request.value,
                },
                Some(expect_version),
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::UpdateLabel {
            project,
            item_id,
            label_id,
            request,
        } => state
            .labels
            .update(
                &project,
                item_id,
                label_id,
                dispatch_types::UpdateWorkItemLabelRequest {
                    key: request.key,
                    value: request.value,
                    expect_version: request.expect_version,
                },
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::DeleteLabel {
            project,
            item_id,
            label_id,
            expect_version,
        } => state
            .labels
            .delete(
                &project,
                item_id,
                label_id,
                Some(expect_version),
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::Move {
            project,
            item_id,
            state: item_state,
            expect_version,
        } => state
            .items
            .update(
                crate::backend::projects::ProjectReference::Name(&project),
                item_id,
                dispatch_types::UpdateWorkItemRequest {
                    state: Some(item_state),
                    expect_version: Some(expect_version),
                    ..Default::default()
                },
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::AddRelationship {
            project,
            item_id,
            request,
        } => state
            .relationships
            .create(
                &project,
                item_id,
                request.target_work_item_id,
                request.kind,
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::UpdateRelationship {
            project,
            item_id,
            relationship_id,
            request,
        } => state
            .relationships
            .update(
                &project,
                Some(item_id),
                relationship_id,
                request.kind,
                Default::default(),
            )
            .await
            .map(|_| ()),
        ItemMutation::DeleteRelationship {
            project,
            item_id,
            relationship_id,
        } => state
            .relationships
            .delete(&project, Some(item_id), relationship_id, Default::default())
            .await
            .map(|_| ()),
    };
    result.map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn item_service() -> ItemService {
    leptos::prelude::expect_context()
}

#[cfg(all(test, feature = "ssr"))]
mod backend_adapter_tests {
    use super::*;
    use assertr::prelude::*;
    use dispatch_types::{AuthorType, UiEvent, WorkItemEventType};
    use leptos::reactive::computed::ScopedFuture;

    #[tokio::test]
    async fn discussion_server_functions_use_request_services_and_preserve_history() {
        let (_first_temp, first, source, target) =
            crate::backend::comments::tests::application().await;
        let (_second_temp, second, second_source, _) =
            crate::backend::comments::tests::application().await;
        let owner = Owner::new();
        owner.with(|| provide_context(first.state.clone()));
        let other_owner = Owner::new();
        other_owner.with(|| provide_context(second.state.clone()));
        let mut events = first.state.events.subscribe();
        let mut other_events = second.state.events.subscribe();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::AddComment {
                    project: "demo".into(),
                    item_id: source,
                    request: AddCommentRequest {
                        author_type: AuthorType::User,
                        author_name: None,
                        body: "Leptos context".into(),
                    },
                }))
            })
            .await
            .unwrap();
        assert_that!(&first.state.comments.list("demo", source).await.unwrap()[0].body)
            .is_equal_to("Leptos context");
        assert_that!(
            &second
                .state
                .comments
                .list("demo", second_source)
                .await
                .unwrap()
        )
        .is_empty();
        assert_that!(&matches!(
            events.try_recv().unwrap(),
            UiEvent::CommentChanged { .. }
        ))
        .is_true();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::AddRelationship {
                    project: "demo".into(),
                    item_id: source,
                    request: CreateWorkItemRelationshipRequest {
                        target_work_item_id: target,
                        kind: " blocks ".into(),
                    },
                }))
            })
            .await
            .unwrap();
        let relationship = first
            .state
            .relationships
            .list("demo", source)
            .await
            .unwrap()
            .remove(0)
            .relationship;
        assert_that!(&relationship.kind).is_equal_to("blocks");
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::UpdateRelationship {
                    project: "demo".into(),
                    item_id: target,
                    relationship_id: relationship.id,
                    request: UpdateWorkItemRelationshipRequest {
                        kind: " follows ".into(),
                    },
                }))
            })
            .await
            .unwrap();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::DeleteRelationship {
                    project: "demo".into(),
                    item_id: source,
                    relationship_id: relationship.id,
                }))
            })
            .await
            .unwrap();
        assert_that!(
            &first
                .state
                .relationships
                .list("demo", source)
                .await
                .unwrap()
        )
        .is_empty();
        let history = first
            .state
            .items
            .events("demo", Some(source), None)
            .await
            .unwrap();
        for kind in [
            WorkItemEventType::CommentAdded,
            WorkItemEventType::RelationshipCreated,
            WorkItemEventType::RelationshipUpdated,
            WorkItemEventType::RelationshipDeleted,
        ] {
            assert_that!(
                &history
                    .iter()
                    .filter(|event| event.event_type == kind)
                    .count()
            )
            .is_equal_to(1);
        }
        let mut changed = vec![];
        while let Ok(event) = events.try_recv() {
            match event {
                UiEvent::WorkItemChanged { item_id, .. } => changed.push(item_id),
                other => panic!("unexpected event: {other:?}"),
            }
        }
        assert_that!(&changed).is_equal_to(vec![source, target, source, target, source, target]);
        assert_that!(&other_events.try_recv().is_err()).is_true();
    }
    #[tokio::test]
    async fn item_and_label_server_functions_share_versions_and_preserve_request_isolation() {
        let (_temp, app, item_id, _) = crate::backend::comments::tests::application().await;
        let (_other_temp, other, other_id, _) =
            crate::backend::comments::tests::application().await;
        let owner = Owner::new();
        owner.with(|| provide_context(app.state.clone()));
        let other_owner = Owner::new();
        other_owner.with(|| provide_context(other.state.clone()));
        let mut events = app.state.events.subscribe();
        let mut other_events = other.state.events.subscribe();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::Move {
                    project: "demo".into(),
                    item_id,
                    state: "review".into(),
                    expect_version: 1,
                }))
            })
            .await
            .unwrap();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::AddLabel {
                    project: "demo".into(),
                    item_id,
                    request: CreateWorkItemLabelRequest {
                        key: "priority".into(),
                        value: Some("high".into()),
                    },
                    expect_version: 2,
                }))
            })
            .await
            .unwrap();
        let label_id = app
            .state
            .labels
            .list("demo", item_id)
            .await
            .unwrap()
            .into_iter()
            .find(|label| label.key == "priority")
            .unwrap()
            .id;
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::UpdateLabel {
                    project: "demo".into(),
                    item_id,
                    label_id,
                    request: UpdateWorkItemLabelRequest {
                        key: None,
                        value: Some(Some("low".into())),
                        expect_version: Some(3),
                    },
                }))
            })
            .await
            .unwrap();
        owner
            .with(|| {
                ScopedFuture::new(mutate_item(ItemMutation::DeleteLabel {
                    project: "demo".into(),
                    item_id,
                    label_id,
                    expect_version: 4,
                }))
            })
            .await
            .unwrap();
        let item = app.state.items.get("demo", item_id).await.unwrap();
        assert_that!(&item.version).is_equal_to(5);
        assert_that!(&item.state.as_deref()).is_equal_to(Some("review"));
        assert_that!(&item.labels.iter().any(|label| label.key == "priority")).is_false();
        let history = app
            .state
            .items
            .events("demo", Some(item_id), None)
            .await
            .unwrap();
        assert_that!(
            &history
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>()
        )
        .is_equal_to(vec![
            WorkItemEventType::ItemCreated,
            WorkItemEventType::ItemMoved,
            WorkItemEventType::LabelAdded,
            WorkItemEventType::LabelUpdated,
            WorkItemEventType::LabelDeleted,
        ]);
        for _ in 0..4 {
            assert_that!(&matches!(events.try_recv().unwrap(), UiEvent::WorkItemChanged {item_id:id,..} if id==item_id)).is_true();
        }
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(&other_events.try_recv().is_err()).is_true();
        assert_that!(
            &other
                .state
                .items
                .get("demo", other_id)
                .await
                .unwrap()
                .version
        )
        .is_equal_to(1);
    }
}
