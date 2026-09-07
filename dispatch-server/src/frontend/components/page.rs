use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_query_map};

pub(crate) fn selected_project_signal() -> Memo<Option<String>> {
    let query = use_query_map();
    let location = use_location();
    Memo::new(move |_| {
        project_from_path(&location.pathname.get()).or_else(|| query.read().get("project"))
    })
}

// Layouts cannot read a child route's named parameters. Resolve the project
// from the same canonical URL for the layout, dock, subscriptions, and pages.
fn project_from_path(path: &str) -> Option<String> {
    let segments = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
    let project = match segments.as_slice() {
        ["", "projects", project, "items", _]
        | ["", "projects", project, "automation", "runs", _, "log"] => *project,
        _ => return None,
    };
    if project.is_empty() {
        return None;
    }
    urlencoding::decode(project)
        .ok()
        .map(|project| project.into_owned())
}

#[cfg(test)]
mod tests {
    use super::project_from_path;
    use assertr::prelude::*;
    #[test]
    fn layout_project_context_recognizes_only_project_scoped_routes() {
        for path in [
            "/projects/demo/items/42",
            "/projects/demo/automation/runs/7/log/",
        ] {
            assert_that!(project_from_path(path)).is_equal_to(Some("demo".to_owned()));
        }
        assert_that!(project_from_path("/projects/space%20and%2Bplus/items/42"))
            .is_equal_to(Some("space and+plus".to_owned()));
        for path in [
            "/",
            "/projects",
            "/projects/demo",
            "/projects//items/42",
            "/projects/demo/settings",
        ] {
            assert_that!(project_from_path(path)).is_none();
        }
    }
}

#[component]
pub(crate) fn QueryFeedback(
    pending: Signal<bool>,
    error: Signal<Option<String>>,
    refresh: Callback<()>,
) -> impl IntoView {
    let state = RwSignal::new((false, None::<String>));
    Effect::new(move |_| state.set((pending.get(), error.get())));
    view! {
        <div class="query-feedback" aria-busy=move || state.get().0>
            {move || state.get().1.map(|error| view! {
                <p role="alert">{error} " " <button type="button" on:click=move |_| refresh.run(())>"Retry"</button></p>
            })}
        </div>
    }
}

/// Server updates advance the baseline; only clean fields adopt the new saved value.
pub(crate) fn sync_saved_value<T>(saved: Signal<T>, draft: ReadSignal<T>, set_draft: WriteSignal<T>)
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let baseline = RwSignal::new(draft.get_untracked());
    Effect::new(move |_| {
        let value = saved.get();
        if draft.get_untracked() == baseline.get_untracked() {
            set_draft.set(value.clone());
        }
        baseline.set(value);
    });
}
