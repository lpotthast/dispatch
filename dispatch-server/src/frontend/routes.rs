use leptos_routes::routes;

#[routes]
pub mod routes {
    use crate::frontend::{
        MainLayout, PageApiDocs, PageBoard, PageErr404, PageError, PageItem, PageProject,
        PageProjects, PageRunLog, PageRuns, PageSystem, PageTriggers,
    };

    fallback!(PageErr404);
    layout!(MainLayout);
    index!(PageBoard);

    #[route("/projects")]
    mod projects {
        page!(PageProjects);
    }

    #[route("/project")]
    mod project {
        page!(PageProject);
    }

    #[route("/automation")]
    mod automation {
        page!(PageTriggers);
    }

    #[route("/runs")]
    mod runs {
        page!(PageRuns);
    }

    #[route("/system")]
    mod system {
        page!(PageSystem);
    }

    #[route("/codex")]
    mod legacy_codex {
        page!(PageSystem);
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
