#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::request::{ServiceFuture, ServiceRequest};
use dispatch_types::ApiDocsPage;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct ApiDocsService {
    load_page: ServiceRequest<Option<String>, ApiDocsPage>,
}

impl ApiDocsService {
    pub(crate) fn new(
        http: crate::frontend::http::HttpService,
        load_page: impl Fn(Option<String>) -> ServiceFuture<ApiDocsPage> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: http.request(load_page),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(http, |selected_project| {
            Box::pin(load_api_docs_page(selected_project))
        })
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<ApiDocsPage, ServerFnError> {
        self.load_page.execute(selected_project).await
    }
}

#[server(prefix = "/leptos")]
async fn load_api_docs_page(
    selected_project: Option<String>,
) -> Result<ApiDocsPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .api_docs_page(selected_project.as_deref())
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn api_docs_service() -> ApiDocsService {
    leptos::prelude::expect_context()
}
