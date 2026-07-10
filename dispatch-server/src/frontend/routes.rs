use leptos_routes::routes;

#[routes]
pub mod routes {
    use crate::frontend::{
        MainLayout, PageApiDocs, PageBoard, PageCodex, PageErr404, PageError, PageItem,
        PageProjects, PageRunLog, PageRuns, PageTriggers,
    };

    fallback!(PageErr404);
    layout!(MainLayout);
    index!(PageBoard);

    #[route("/projects")]
    mod projects {
        page!(PageProjects);
    }

    #[route("/automation")]
    mod automation {
        page!(PageTriggers);
    }

    #[route("/runs")]
    mod runs {
        page!(PageRuns);
    }

    #[route("/codex")]
    mod codex {
        page!(PageCodex);
    }

    #[route("/api/docs")]
    mod api_docs {
        page!(PageApiDocs);
    }

    #[route("/error")]
    mod error {
        page!(PageError);
    }

    #[route("/projects/:project/items/:item_id")]
    mod item {
        page!(PageItem);
    }

    #[route("/projects/:project/automation/runs/:run_id/log")]
    mod run_log {
        page!(PageRunLog);
    }
}
