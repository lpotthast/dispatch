#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::request::{ServiceFuture, ServiceRequest};
use dispatch_types::MetricsPageData;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct MetricsService {
    load_page: ServiceRequest<Option<String>, MetricsPageData>,
}

impl MetricsService {
    pub(crate) fn new(
        http: crate::frontend::http::HttpService,
        load_page: impl Fn(Option<String>) -> ServiceFuture<MetricsPageData> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: http.request(load_page),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(http, |selected_project| {
            Box::pin(load_metrics_page(selected_project))
        })
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<MetricsPageData, ServerFnError> {
        self.load_page.execute(selected_project).await
    }
}

#[server(prefix = "/leptos")]
async fn load_metrics_page(
    selected_project: Option<String>,
) -> Result<MetricsPageData, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    crate::backend::metrics::time_repository(
        "metrics.page",
        state
            .operator_queries
            .metrics_page(selected_project.as_deref()),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn metrics_service() -> MetricsService {
    leptos::prelude::expect_context()
}
