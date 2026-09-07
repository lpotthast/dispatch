#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::{
    origin::api_base_url,
    request::{ServiceFuture, ServiceRequest},
};
use dispatch_types::{BoardItemsSection, BoardPage};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct BoardService {
    load_page: ServiceRequest<Option<String>, BoardPage>,
    load_items: ServiceRequest<String, BoardItemsSection>,
}

impl BoardService {
    pub(crate) fn new(
        http: crate::frontend::http::HttpService,
        load_page: impl Fn(Option<String>) -> ServiceFuture<BoardPage> + Send + Sync + 'static,
        load_items: impl Fn(String) -> ServiceFuture<BoardItemsSection> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: http.request(load_page),
            load_items: http.request(load_items),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(
            http.clone(),
            |selected_project| Box::pin(load_board_page(selected_project, api_base_url())),
            |project| Box::pin(load_board_items_section(project)),
        )
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<BoardPage, ServerFnError> {
        self.load_page.execute(selected_project).await
    }

    pub(crate) async fn load_items(
        &self,
        project: String,
    ) -> Result<BoardItemsSection, ServerFnError> {
        self.load_items.execute(project).await
    }
}

#[server(prefix = "/leptos")]
async fn load_board_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<BoardPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    crate::backend::metrics::time_repository(
        "board.page",
        state
            .board_queries
            .page(selected_project.as_deref(), api_base_url),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_board_items_section(project: String) -> Result<BoardItemsSection, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    crate::backend::metrics::time_repository(
        "board.items_section",
        state.board_queries.items_section(&project),
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn board_service() -> BoardService {
    leptos::prelude::expect_context()
}

#[cfg(all(test, feature = "ssr"))]
mod backend_query_tests {
    use super::*;
    use assertr::prelude::*;
    use leptos::reactive::computed::ScopedFuture;
    #[tokio::test]
    async fn page_and_section_adapters_share_typed_queries_and_keep_request_instances_separate() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let other_temp = tempfile::tempdir().unwrap();
        let other = crate::backend::application::Application::open(
            other_temp.path().join("other.sqlite3"),
            "http://127.0.0.1:4102".into(),
        )
        .await
        .unwrap();
        let owner = Owner::new();
        owner.with(|| provide_context(app.state.clone()));
        let other_owner = Owner::new();
        other_owner.with(|| provide_context(other.state.clone()));
        let expected = app
            .state
            .board_queries
            .page(Some("demo"), "http://example.test".into())
            .await
            .unwrap();
        let page = owner
            .with(|| {
                ScopedFuture::new(load_board_page(
                    Some("demo".into()),
                    "http://example.test".into(),
                ))
            })
            .await
            .unwrap();
        assert_that!(&page).is_equal_to(expected);
        let section = owner
            .with(|| ScopedFuture::new(load_board_items_section("demo".into())))
            .await
            .unwrap();
        assert_that!(&section.items).is_equal_to(page.items);
        assert_that!(&section.swim_lanes).is_equal_to(page.swim_lanes);
        assert_that!(&section.work_item_states).is_equal_to(page.work_item_states);
        assert_that!(&section.label_accent_colors).is_equal_to(page.label_accent_colors);
        let empty = other_owner
            .with(|| {
                ScopedFuture::new(load_board_page(
                    Some("demo".into()),
                    "http://other.test".into(),
                ))
            })
            .await
            .unwrap();
        assert_that!(&empty.projects).is_empty();
        assert_that!(&empty.selected_project).is_none();
        assert_that!(&empty.items).is_empty();
        assert_that!(&empty.api_base_url.as_str()).is_equal_to("http://other.test");
    }
}
