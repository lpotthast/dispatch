#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::services::{
    cache::LocalStorageCache,
    origin::api_base_url,
    request::{ServiceFuture, ServiceRequest},
};
use crate::shared::page_data::{BoardItemsSection, BoardPage};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct BoardService {
    load_page: ServiceRequest<Option<String>, BoardPage>,
    load_items: ServiceRequest<String, BoardItemsSection>,
    page_cache: Option<LocalStorageCache<BoardPage>>,
    items_cache: Option<LocalStorageCache<BoardItemsSection>>,
}

impl BoardService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<BoardPage> + Send + Sync + 'static,
        load_items: impl Fn(String) -> ServiceFuture<BoardItemsSection> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            load_items: ServiceRequest::new(load_items),
            page_cache: None,
            items_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_board_page(selected_project, api_base_url())),
            |project| Box::pin(load_board_items_section(project)),
        );
        service.page_cache = Some(LocalStorageCache::persistent("dispatch.query.board.v3"));
        service.items_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.board-items.v3",
        ));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<BoardPage> {
        self.page_cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<BoardPage> {
        self.page_cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<BoardPage, ServerFnError> {
        let lifecycle_epoch = self.page_cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.page_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) {
            if let (Some(cache), Some((project, section))) =
                (self.items_cache, board_items_section_from_page(&page))
            {
                cache.store_owned(&project, section);
            }
            if let Some(cache) = self.page_cache {
                cache.store_owned(&key, board_page_shell(&page));
            }
        }
        Ok(page)
    }

    pub(crate) fn cached_items(&self, project: &str) -> Option<BoardItemsSection> {
        self.items_cache?.get(&project)
    }

    pub(crate) fn cached_items_untracked(&self, project: &str) -> Option<BoardItemsSection> {
        self.items_cache?.get_untracked(&project)
    }

    pub(crate) async fn load_items(
        &self,
        project: String,
    ) -> Result<BoardItemsSection, ServerFnError> {
        let lifecycle_epoch = self
            .items_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let items = self.load_items.execute(project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.items_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.items_cache
        {
            cache.store(&key, &items);
        }
        Ok(items)
    }

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.page_cache {
            cache.clear();
        }
        if let Some(cache) = self.items_cache {
            cache.clear();
        }
    }
}

fn board_items_section_from_page(page: &BoardPage) -> Option<(String, BoardItemsSection)> {
    Some((
        page.selected_project.clone()?,
        BoardItemsSection {
            items: page.items.clone(),
            swim_lanes: page.swim_lanes.clone(),
            work_item_states: page.work_item_states.clone(),
            label_accent_colors: page.label_accent_colors.clone(),
            misconfigured_item_count: page.misconfigured_item_count,
        },
    ))
}

fn board_page_shell(page: &BoardPage) -> BoardPage {
    BoardPage {
        projects: page.projects.clone(),
        active_project_names: page.active_project_names.clone(),
        selected_project: page.selected_project.clone(),
        selected_project_view: page.selected_project_view.clone(),
        automation_status: page.automation_status.clone(),
        automation_running: page.automation_running,
        items: Vec::new(),
        swim_lanes: Vec::new(),
        work_item_states: Vec::new(),
        label_suggestions: page.label_suggestions.clone(),
        label_accent_colors: Default::default(),
        misconfigured_item_count: 0,
        api_base_url: page.api_base_url.clone(),
        codex_status: page.codex_status.clone(),
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
