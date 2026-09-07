mod jobs;
use crate::frontend::http::request::ServiceRequest;
use dispatch_types::knowledge::{
    KnowledgeOperation, KnowledgeQuery, KnowledgeSaveRequest, KnowledgeSaveResult, KnowledgeView,
};
pub(crate) use jobs::KnowledgeJobsUiService;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct KnowledgeUiService {
    pub(crate) jobs: KnowledgeJobsUiService,
    read: ServiceRequest<(String, KnowledgeOperation, KnowledgeQuery), KnowledgeView>,
    save: ServiceRequest<(String, KnowledgeSaveRequest), KnowledgeSaveResult>,
}
impl KnowledgeUiService {
    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self {
            jobs: KnowledgeJobsUiService::production(http.clone()),
            save: http.request(|(project, request)| Box::pin(save_knowledge(project, request))),
            read: http.request(|(project, operation, query)| {
                Box::pin(read_knowledge(project, operation, query))
            }),
        }
    }
    pub(crate) async fn save(
        &self,
        project: String,
        request: KnowledgeSaveRequest,
    ) -> Result<KnowledgeSaveResult, ServerFnError> {
        self.save.execute_inline((project, request)).await
    }

    /// Fetch graph summaries without document bodies and reject mixed-generation pages.
    pub(crate) async fn graph(&self, project: String) -> Result<KnowledgeView, ServerFnError> {
        let mut view = self
            .read(
                project.clone(),
                KnowledgeOperation::Graph,
                KnowledgeQuery {
                    limit: Some(100),
                    ..Default::default()
                },
            )
            .await?;
        while let Some(offset) = view.next_offset {
            let next = self
                .read(
                    project.clone(),
                    KnowledgeOperation::Graph,
                    KnowledgeQuery {
                        offset,
                        limit: Some(100),
                        ..Default::default()
                    },
                )
                .await?;
            if view.index_generation != next.index_generation {
                return Err(ServerFnError::new(
                    "Knowledge changed while loading the graph. Refresh to read current files.",
                ));
            }
            view.documents.extend(next.documents);
            view.relations.extend(next.relations);
            view.next_offset = next.next_offset;
        }
        Ok(view)
    }

    /// Collect a complete editor body, never treating a truncated page as the whole file.
    pub(crate) async fn document(
        &self,
        project: String,
        path: String,
    ) -> Result<dispatch_types::knowledge::KnowledgeDocument, ServerFnError> {
        let mut view = self
            .read(
                project.clone(),
                KnowledgeOperation::Node,
                KnowledgeQuery {
                    path: Some(path.clone()),
                    ..Default::default()
                },
            )
            .await?;
        let mut document = view
            .document
            .take()
            .ok_or_else(|| ServerFnError::new("Document is missing"))?;
        while let Some(body_offset) = document.next_body_offset {
            let next = self
                .read(
                    project.clone(),
                    KnowledgeOperation::Node,
                    KnowledgeQuery {
                        path: Some(path.clone()),
                        body_offset,
                        ..Default::default()
                    },
                )
                .await?;
            let next = next
                .document
                .ok_or_else(|| ServerFnError::new("Document is missing"))?;
            if next.summary.fingerprint != document.summary.fingerprint {
                return Err(ServerFnError::new(
                    "Document changed while reading. Refresh to retry.",
                ));
            }
            document.markdown.push_str(&next.markdown);
            document.next_body_offset = next.next_body_offset;
        }
        Ok(document)
    }

    pub(crate) async fn read(
        &self,
        project: String,
        operation: KnowledgeOperation,
        query: KnowledgeQuery,
    ) -> Result<KnowledgeView, ServerFnError> {
        self.read.execute_inline((project, operation, query)).await
    }
}
pub(crate) fn knowledge_ui_service() -> KnowledgeUiService {
    expect_context()
}
#[server(prefix = "/leptos")]
async fn read_knowledge(
    project: String,
    operation: KnowledgeOperation,
    query: KnowledgeQuery,
) -> Result<KnowledgeView, ServerFnError> {
    let state = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    state
        .knowledge_queries
        .query(
            &project,
            crate::backend::attribution::model::AttributionInput::default(),
            operation,
            query,
        )
        .await
        .map_err(|error| ServerFnError::new(error.as_ref().format_current_context().to_string()))
}

#[server(prefix = "/leptos")]
async fn save_knowledge(
    project: String,
    request: KnowledgeSaveRequest,
) -> Result<KnowledgeSaveResult, ServerFnError> {
    let state = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    state
        .knowledge
        .save(&project, request)
        .await
        .map_err(|error| ServerFnError::new(error.as_ref().format_current_context().to_string()))
}

#[cfg(all(test, feature = "ssr"))]
mod backend_adapter_tests {
    use super::*;
    use assertr::prelude::*;
    use leptos::reactive::computed::ScopedFuture;
    #[tokio::test]
    async fn document_adapters_share_current_files_and_checked_saves_without_jobs() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let owner = Owner::new();
        owner.with(|| provide_context(app.state.clone()));
        let request = KnowledgeSaveRequest {
            path: "README.md".into(),
            expected_fingerprint: None,
            markdown: "---\nid: example\n---\n# Example\n\nAccepted contract.\n".into(),
        };
        let saved = owner
            .with(|| ScopedFuture::new(save_knowledge("demo".into(), request.clone())))
            .await
            .unwrap();
        assert_that!(&saved.fingerprint.is_empty()).is_false();
        let read = owner
            .with(|| {
                ScopedFuture::new(read_knowledge(
                    "demo".into(),
                    KnowledgeOperation::Root,
                    KnowledgeQuery::default(),
                ))
            })
            .await
            .unwrap();
        assert_that!(&read).is_equal_to(
            app.state
                .knowledge
                .query(
                    "demo",
                    Default::default(),
                    KnowledgeOperation::Root,
                    KnowledgeQuery::default(),
                )
                .await
                .unwrap(),
        );
        assert_that!(&read.document.unwrap().markdown).is_equal_to(request.markdown.clone());
        assert_that!(
            &owner
                .with(|| ScopedFuture::new(save_knowledge("demo".into(), request)))
                .await
                .is_err()
        )
        .is_true();
        assert_that!(&app.state.jobs.list("demo").await.unwrap()).is_empty();
    }
}
