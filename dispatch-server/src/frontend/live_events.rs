#[cfg(not(feature = "ssr"))]
use crate::frontend::services::{
    advance_project_lifecycle_epoch, api_docs_service, automation_service, board_service,
    codex_service, item_service, run_service,
};
#[cfg(not(feature = "ssr"))]
use crate::frontend::services::{project_cache, project_service};
use crate::shared::view_models::UiEvent;
#[cfg(not(feature = "ssr"))]
use codee::string::FromToStringCodec;
use crudkit_leptos::crud_instance::CrudInstanceContext;
use leptos::prelude::*;
#[cfg(not(feature = "ssr"))]
use leptos_router::{
    NavigateOptions,
    hooks::{use_navigate, use_params_map, use_query_map},
};
#[cfg(not(feature = "ssr"))]
use leptos_use::{
    ReconnectLimit, UseWebSocketOptions, UseWebSocketReturn, use_websocket_with_options,
};

#[derive(Clone, Copy)]
struct LiveEventContext {
    latest_event: ReadSignal<Option<UiEvent>>,
}

#[component]
pub(crate) fn LiveEventsProvider() -> impl IntoView {
    let (latest_event, set_latest_event) = signal(None::<UiEvent>);
    provide_context(LiveEventContext { latest_event });
    #[cfg(feature = "ssr")]
    let _ = set_latest_event;

    #[cfg(not(feature = "ssr"))]
    {
        let navigate = use_navigate();
        let query = use_query_map();
        let params = use_params_map();
        let project_service = project_service();
        let project_cache = project_cache();
        let board_service = board_service();
        let automation_service = automation_service();
        let item_service = item_service();
        let run_service = run_service();
        let codex_service = codex_service();
        let api_docs_service = api_docs_service();
        let UseWebSocketReturn { message, .. } =
            use_websocket_with_options::<String, String, FromToStringCodec, _, _>(
                "/api/events/ws",
                UseWebSocketOptions::default()
                    .reconnect_limit(ReconnectLimit::Infinite)
                    .reconnect_interval(1_000),
            );
        Effect::new(move |_| {
            if let Some(raw) = message.get()
                && let Ok(event) = serde_json::from_str::<UiEvent>(&raw)
            {
                set_latest_event.set(Some(event));
            }
        });
        Effect::new(move |_| {
            let Some(UiEvent::ProjectDeleted {
                project_id,
                project,
                ..
            }) = latest_event.get()
            else {
                return;
            };
            advance_project_lifecycle_epoch();
            project_service.clear_cache();
            board_service.clear_cache();
            automation_service.clear_cache();
            item_service.clear_cache();
            run_service.clear_cache();
            codex_service.clear_cache();
            api_docs_service.clear_cache();
            project_cache.remove(project_id, &project);
            let selected = query
                .read()
                .get("project")
                .or_else(|| params.read().get("project"));
            if selected.as_deref() == Some(project.as_str()) {
                navigate(
                    "/projects",
                    NavigateOptions {
                        replace: true,
                        scroll: true,
                        ..NavigateOptions::default()
                    },
                );
            }
        });
    }
}

pub(crate) fn refetch_on_live_event(
    refresh: Callback<()>,
    should_refetch: impl Fn(&UiEvent) -> bool + 'static,
) {
    if let Some(context) = use_context::<LiveEventContext>() {
        Effect::new(move |_| {
            if let Some(event) = context.latest_event.get()
                && should_refetch(&event)
            {
                refresh.run(());
            }
        });
    }
}

pub(crate) fn reload_crudkit_on_live_event(
    context: ReadSignal<Option<CrudInstanceContext>>,
    should_reload: impl Fn(&UiEvent) -> bool + 'static,
) {
    if let Some(live) = use_context::<LiveEventContext>() {
        Effect::new(move |_| {
            if let Some(event) = live.latest_event.get()
                && should_reload(&event)
                && let Some(context) = context.get()
            {
                context.reload();
            }
        });
    }
}

pub(crate) fn event_scopes_named_project(event: &UiEvent, project: Option<&str>) -> bool {
    match (project, event_project(event)) {
        (Some(expected), Some(actual)) => expected == actual,
        (Some(_), None) => true,
        (None, _) => true,
    }
}

pub(crate) fn codex_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::CodexStatusChanged { .. }
            | UiEvent::AgentToolChanged { .. }
            | UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
            | UiEvent::ProjectDeleted { .. }
            | UiEvent::AutomationChanged { .. }
    )
}

pub(crate) fn projects_page_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
            | UiEvent::ProjectDeleted { .. }
            | UiEvent::AutomationChanged { .. }
            | UiEvent::CodexStatusChanged { .. }
    )
}

pub(crate) fn api_docs_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
            | UiEvent::ProjectDeleted { .. }
            | UiEvent::CodexStatusChanged { .. }
    )
}

pub(crate) fn runs_page_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
            | UiEvent::ProjectDeleted { .. }
            | UiEvent::CodexStatusChanged { .. }
    )
}

pub(crate) fn item_event_matches(
    event: &UiEvent,
    project: Option<&str>,
    item_id: Option<i64>,
) -> bool {
    if !event_scopes_named_project(event, project) {
        return false;
    }
    match event {
        UiEvent::ProjectListChanged { .. }
        | UiEvent::ProjectChanged { .. }
        | UiEvent::ProjectDeleted { .. }
        | UiEvent::AutomationChanged { .. }
        | UiEvent::CodexStatusChanged { .. }
        | UiEvent::AgentToolChanged { .. } => true,
        UiEvent::WorkItemChanged {
            item_id: changed_item_id,
            ..
        }
        | UiEvent::CommentChanged {
            item_id: changed_item_id,
            ..
        } => Some(*changed_item_id) == item_id,
        UiEvent::AgentRunChanged {
            item_id: Some(changed_item_id),
            ..
        }
        | UiEvent::AgentOutputChanged {
            item_id: Some(changed_item_id),
            ..
        } => Some(*changed_item_id) == item_id,
        UiEvent::AgentRunChanged { item_id: None, .. }
        | UiEvent::AgentOutputChanged { item_id: None, .. }
        | UiEvent::SystemPromptChanged { .. }
        | UiEvent::MemoryChanged { .. }
        | UiEvent::SwimLaneChanged { .. } => false,
        UiEvent::WorkItemStateChanged { .. } => true,
    }
}

pub(crate) fn run_log_event_matches(
    event: &UiEvent,
    project: Option<&str>,
    run_id: Option<i64>,
) -> bool {
    if !event_scopes_named_project(event, project) {
        return false;
    }
    match event {
        UiEvent::AgentRunChanged {
            run_id: changed_run_id,
            ..
        }
        | UiEvent::AgentOutputChanged {
            run_id: changed_run_id,
            ..
        } => Some(*changed_run_id) == run_id,
        UiEvent::ProjectListChanged { .. }
        | UiEvent::ProjectChanged { .. }
        | UiEvent::ProjectDeleted { .. }
        | UiEvent::AutomationChanged { .. }
        | UiEvent::CodexStatusChanged { .. }
        | UiEvent::AgentToolChanged { .. } => true,
        UiEvent::WorkItemChanged { .. }
        | UiEvent::CommentChanged { .. }
        | UiEvent::SystemPromptChanged { .. }
        | UiEvent::MemoryChanged { .. }
        | UiEvent::SwimLaneChanged { .. }
        | UiEvent::WorkItemStateChanged { .. } => false,
    }
}

pub(crate) fn board_items_event_matches(event: &UiEvent, project: &str) -> bool {
    event_scopes_named_project(event, Some(project))
        && matches!(
            event,
            UiEvent::WorkItemChanged { .. }
                | UiEvent::CommentChanged { .. }
                | UiEvent::AgentRunChanged { .. }
                | UiEvent::SwimLaneChanged { .. }
                | UiEvent::WorkItemStateChanged { .. }
        )
}

pub(crate) fn runs_section_event_matches(event: &UiEvent, project: &str) -> bool {
    event_scopes_named_project(event, Some(project))
        && matches!(
            event,
            UiEvent::AutomationChanged { .. }
                | UiEvent::AgentRunChanged { .. }
                | UiEvent::AgentOutputChanged { .. }
                | UiEvent::CodexStatusChanged { .. }
        )
}

pub(crate) fn trigger_runs_event_matches(event: &UiEvent, project: &str) -> bool {
    runs_section_event_matches(event, project)
}

fn event_project(event: &UiEvent) -> Option<&str> {
    match event {
        UiEvent::ProjectChanged { project, .. }
        | UiEvent::ProjectDeleted { project, .. }
        | UiEvent::SystemPromptChanged { project, .. }
        | UiEvent::WorkItemChanged { project, .. }
        | UiEvent::CommentChanged { project, .. }
        | UiEvent::MemoryChanged { project, .. }
        | UiEvent::SwimLaneChanged { project, .. }
        | UiEvent::WorkItemStateChanged { project, .. }
        | UiEvent::AutomationChanged { project, .. }
        | UiEvent::AgentRunChanged { project, .. }
        | UiEvent::AgentOutputChanged { project, .. } => Some(project),
        UiEvent::ProjectListChanged { .. }
        | UiEvent::AgentToolChanged { .. }
        | UiEvent::CodexStatusChanged { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        api_docs_event_matches, board_items_event_matches, codex_event_matches,
        event_scopes_named_project, item_event_matches, projects_page_event_matches,
        run_log_event_matches, runs_page_event_matches, runs_section_event_matches,
        trigger_runs_event_matches,
    };
    use crate::shared::view_models::UiEvent;
    use assertr::prelude::*;

    const DEMO_PROJECT: &str = "demo";
    const OTHER_PROJECT: &str = "other";
    const TIMESTAMP: &str = "2026-06-18T00:00:00Z";

    fn timestamp() -> String {
        TIMESTAMP.to_owned()
    }

    fn project_list_changed() -> UiEvent {
        UiEvent::ProjectListChanged {
            sequence: 1,
            timestamp: timestamp(),
        }
    }

    fn project_changed(project: &str) -> UiEvent {
        UiEvent::ProjectChanged {
            sequence: 2,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn system_prompt_changed(project: &str) -> UiEvent {
        UiEvent::SystemPromptChanged {
            sequence: 3,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn work_item_changed(project: &str, item_id: i64) -> UiEvent {
        UiEvent::WorkItemChanged {
            sequence: 4,
            timestamp: timestamp(),
            project: project.to_owned(),
            item_id,
        }
    }

    fn comment_changed(project: &str, item_id: i64) -> UiEvent {
        UiEvent::CommentChanged {
            sequence: 5,
            timestamp: timestamp(),
            project: project.to_owned(),
            item_id,
        }
    }

    fn memory_changed(project: &str) -> UiEvent {
        UiEvent::MemoryChanged {
            sequence: 6,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn swim_lane_changed(project: &str) -> UiEvent {
        UiEvent::SwimLaneChanged {
            sequence: 7,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn work_item_state_changed(project: &str) -> UiEvent {
        UiEvent::WorkItemStateChanged {
            sequence: 8,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn agent_tool_changed() -> UiEvent {
        UiEvent::AgentToolChanged {
            sequence: 9,
            timestamp: timestamp(),
        }
    }

    fn automation_changed(project: &str) -> UiEvent {
        UiEvent::AutomationChanged {
            sequence: 10,
            timestamp: timestamp(),
            project: project.to_owned(),
        }
    }

    fn agent_run_changed(project: &str, run_id: i64, item_id: Option<i64>) -> UiEvent {
        UiEvent::AgentRunChanged {
            sequence: 11,
            timestamp: timestamp(),
            project: project.to_owned(),
            run_id,
            item_id,
        }
    }

    fn agent_output_changed(project: &str, run_id: i64, item_id: Option<i64>) -> UiEvent {
        UiEvent::AgentOutputChanged {
            sequence: 12,
            timestamp: timestamp(),
            project: project.to_owned(),
            run_id,
            item_id,
        }
    }

    fn codex_status_changed() -> UiEvent {
        UiEvent::CodexStatusChanged {
            sequence: 13,
            timestamp: timestamp(),
        }
    }

    #[test]
    fn event_scope_matches_named_project_only_for_project_events() {
        let event = work_item_changed(DEMO_PROJECT, 7);

        assert_that!(&(event_scopes_named_project(&event, Some(DEMO_PROJECT)))).is_true();
        assert_that!(&(!event_scopes_named_project(&event, Some(OTHER_PROJECT)))).is_true();
        assert_that!(&(event_scopes_named_project(&event, None))).is_true();
    }

    #[test]
    fn event_scope_keeps_global_events_matching_selected_project() {
        for event in [
            project_list_changed(),
            agent_tool_changed(),
            codex_status_changed(),
        ] {
            assert_that!(&(event_scopes_named_project(&event, Some(DEMO_PROJECT)))).is_true();
            assert_that!(&(event_scopes_named_project(&event, None))).is_true();
        }
    }

    #[test]
    fn codex_page_refreshes_for_global_events() {
        assert_that!(&(codex_event_matches(&project_list_changed()))).is_true();
        assert_that!(&(codex_event_matches(&agent_tool_changed()))).is_true();
        assert_that!(&(codex_event_matches(&codex_status_changed()))).is_true();
    }

    #[test]
    fn projects_page_refreshes_for_project_lifecycle_and_shell_events() {
        assert_that!(&(projects_page_event_matches(&project_list_changed()))).is_true();
        assert_that!(&(projects_page_event_matches(&project_changed(DEMO_PROJECT)))).is_true();
        assert_that!(&(projects_page_event_matches(&automation_changed(DEMO_PROJECT)))).is_true();
        assert_that!(&(projects_page_event_matches(&codex_status_changed()))).is_true();
        assert_that!(&(!projects_page_event_matches(&agent_tool_changed()))).is_true();
    }

    #[test]
    fn api_docs_and_runs_pages_refresh_for_shell_context_events() {
        assert_that!(&(api_docs_event_matches(&project_list_changed()))).is_true();
        assert_that!(&(api_docs_event_matches(&project_changed(DEMO_PROJECT)))).is_true();
        assert_that!(&(api_docs_event_matches(&codex_status_changed()))).is_true();
        assert_that!(&(!api_docs_event_matches(&agent_tool_changed()))).is_true();

        assert_that!(&(runs_page_event_matches(&project_list_changed()))).is_true();
        assert_that!(&(runs_page_event_matches(&project_changed(DEMO_PROJECT)))).is_true();
        assert_that!(&(runs_page_event_matches(&codex_status_changed()))).is_true();
        assert_that!(&(!runs_page_event_matches(&agent_tool_changed()))).is_true();
    }

    #[test]
    fn runs_page_shell_ignores_live_run_events() {
        assert_that!(&(!runs_page_event_matches(&automation_changed(DEMO_PROJECT)))).is_true();
        assert_that!(&(!runs_page_event_matches(&agent_run_changed(DEMO_PROJECT, 42, Some(7)))))
            .is_true();
        assert_that!(&(!runs_page_event_matches(&agent_output_changed(DEMO_PROJECT, 42, Some(7)))))
            .is_true();
    }

    #[test]
    fn item_detail_refreshes_for_global_events() {
        assert_that!(&(item_event_matches(&project_list_changed(), Some(DEMO_PROJECT), Some(7))))
            .is_true();
        assert_that!(&(item_event_matches(&agent_tool_changed(), Some(DEMO_PROJECT), Some(7))))
            .is_true();
        assert_that!(&(item_event_matches(&codex_status_changed(), Some(DEMO_PROJECT), Some(7))))
            .is_true();
    }

    #[test]
    fn item_detail_matches_selected_item_events() {
        assert_that!(
            &(item_event_matches(
                &work_item_changed(DEMO_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(item_event_matches(
                &comment_changed(DEMO_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(item_event_matches(
                &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(item_event_matches(
                &agent_output_changed(DEMO_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
    }

    #[test]
    fn item_detail_ignores_unmatched_item_and_run_events() {
        assert_that!(
            &(!item_event_matches(
                &work_item_changed(DEMO_PROJECT, 8),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &comment_changed(DEMO_PROJECT, 8),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_run_changed(DEMO_PROJECT, 42, Some(8)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_output_changed(DEMO_PROJECT, 42, Some(8)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_run_changed(DEMO_PROJECT, 99, None),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_output_changed(DEMO_PROJECT, 99, None),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
    }

    #[test]
    fn item_detail_applies_project_scoping_before_item_matching() {
        assert_that!(
            &(!item_event_matches(
                &work_item_changed(OTHER_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &comment_changed(OTHER_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_run_changed(OTHER_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
        assert_that!(
            &(!item_event_matches(
                &agent_output_changed(OTHER_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(7)
            ))
        )
        .is_true();
    }

    #[test]
    fn run_log_refreshes_for_global_events() {
        assert_that!(
            &(run_log_event_matches(&project_list_changed(), Some(DEMO_PROJECT), Some(42)))
        )
        .is_true();
        assert_that!(&(run_log_event_matches(&agent_tool_changed(), Some(DEMO_PROJECT), Some(42))))
            .is_true();
        assert_that!(
            &(run_log_event_matches(&codex_status_changed(), Some(DEMO_PROJECT), Some(42)))
        )
        .is_true();
    }

    #[test]
    fn run_log_matches_selected_run_events() {
        assert_that!(
            &(run_log_event_matches(
                &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(run_log_event_matches(
                &agent_output_changed(DEMO_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
    }

    #[test]
    fn run_log_ignores_unmatched_run_events() {
        assert_that!(
            &(!run_log_event_matches(
                &agent_run_changed(DEMO_PROJECT, 99, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &agent_output_changed(DEMO_PROJECT, 99, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
    }

    #[test]
    fn run_log_ignores_unrelated_item_and_project_content_events() {
        assert_that!(
            &(!run_log_event_matches(
                &work_item_changed(DEMO_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &comment_changed(DEMO_PROJECT, 7),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &system_prompt_changed(DEMO_PROJECT),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(&memory_changed(DEMO_PROJECT), Some(DEMO_PROJECT), Some(42)))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &swim_lane_changed(DEMO_PROJECT),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &work_item_state_changed(DEMO_PROJECT),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
    }

    #[test]
    fn run_log_applies_project_scoping_before_run_matching() {
        assert_that!(
            &(!run_log_event_matches(
                &agent_run_changed(OTHER_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
        assert_that!(
            &(!run_log_event_matches(
                &agent_output_changed(OTHER_PROJECT, 42, Some(7)),
                Some(DEMO_PROJECT),
                Some(42)
            ))
        )
        .is_true();
    }

    #[test]
    fn board_items_matches_board_refresh_events_for_project() {
        assert_that!(
            &(board_items_event_matches(&work_item_changed(DEMO_PROJECT, 7), DEMO_PROJECT))
        )
        .is_true();
        assert_that!(&(board_items_event_matches(&comment_changed(DEMO_PROJECT, 7), DEMO_PROJECT)))
            .is_true();
        assert_that!(
            &(board_items_event_matches(
                &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
                DEMO_PROJECT
            ))
        )
        .is_true();
        assert_that!(&(board_items_event_matches(&swim_lane_changed(DEMO_PROJECT), DEMO_PROJECT)))
            .is_true();
        assert_that!(
            &(board_items_event_matches(&work_item_state_changed(DEMO_PROJECT), DEMO_PROJECT))
        )
        .is_true();
        assert_that!(
            &(!board_items_event_matches(&work_item_changed(OTHER_PROJECT, 7), DEMO_PROJECT))
        )
        .is_true();
        assert_that!(
            &(!board_items_event_matches(&comment_changed(OTHER_PROJECT, 7), DEMO_PROJECT))
        )
        .is_true();
        assert_that!(&(!board_items_event_matches(&codex_status_changed(), DEMO_PROJECT)))
            .is_true();
    }

    #[test]
    fn run_session_sections_match_only_run_refresh_events_for_project() {
        for event in [
            automation_changed(DEMO_PROJECT),
            agent_run_changed(DEMO_PROJECT, 42, Some(7)),
            agent_output_changed(DEMO_PROJECT, 42, Some(7)),
            codex_status_changed(),
        ] {
            assert_that!(&(runs_section_event_matches(&event, DEMO_PROJECT))).is_true();
            assert_that!(&(trigger_runs_event_matches(&event, DEMO_PROJECT))).is_true();
        }

        for event in [
            automation_changed(OTHER_PROJECT),
            agent_run_changed(OTHER_PROJECT, 42, Some(7)),
            agent_output_changed(OTHER_PROJECT, 42, Some(7)),
            work_item_changed(DEMO_PROJECT, 7),
        ] {
            assert_that!(&(!runs_section_event_matches(&event, DEMO_PROJECT))).is_true();
            assert_that!(&(!trigger_runs_event_matches(&event, DEMO_PROJECT))).is_true();
        }
    }
}
