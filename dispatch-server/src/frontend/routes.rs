use leptos_routes::routes;

#[routes]
pub mod routes {
    use crate::frontend::{
        MainLayout, PageApiDocs, PageBoard, PageErr404, PageError, PageItem, PageKnowledge,
        PageMetrics, PageProject, PageProjects, PageRunLog, PageRuns, PageSystem, PageTriggers,
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

    #[route("/knowledge")]
    mod knowledge {
        page!(PageKnowledge);
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

    #[route("/metrics")]
    mod metrics {
        page!(PageMetrics);
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

use leptos_router::params::{IntoParam, Params, ParamsError, ParamsMap};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemParams {
    pub project: Option<String>,
    pub item_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunLogParams {
    pub project: Option<String>,
    pub run_id: Option<i64>,
}

impl Params for ItemParams {
    fn from_map(map: &ParamsMap) -> Result<Self, ParamsError> {
        Ok(Self {
            project: map.get("project"),
            item_id: Option::<i64>::into_param(map.get("item_id").as_deref(), "item_id")?,
        })
    }
}
impl Params for RunLogParams {
    fn from_map(map: &ParamsMap) -> Result<Self, ParamsError> {
        Ok(Self {
            project: map.get("project"),
            run_id: Option::<i64>::into_param(map.get("run_id").as_deref(), "run_id")?,
        })
    }
}

pub(crate) fn with_project(path: String, project: Option<&str>) -> String {
    match project {
        Some(project) => format!("{path}?project={}", urlencoding::encode(project)),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn route_links_encode_project_names_once() {
        assert_that!(routes::Item.materialize(urlencoding::encode("a/b +c"), 42))
            .is_equal_to("/projects/a%2Fb%20%2Bc/items/42");
        assert_that!(with_project(routes::Root.materialize(), Some("a/b +c")))
            .is_equal_to("/?project=a%2Fb%20%2Bc");
        assert_that!(routes::RunLog.materialize("demo", 7))
            .is_equal_to("/projects/demo/automation/runs/7/log");
    }
}
