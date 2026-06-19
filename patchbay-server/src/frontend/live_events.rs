use crate::shared::view_models::UiEvent;
#[cfg(not(feature = "ssr"))]
use codee::string::FromToStringCodec;
use crudkit_leptos::crud_instance::CrudInstanceContext;
use leptos::prelude::*;
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
    }
}

pub(crate) fn refetch_on_live_event<T>(
    resource: LocalResource<T>,
    should_refetch: impl Fn(&UiEvent) -> bool + 'static,
) where
    T: 'static,
{
    if let Some(context) = use_context::<LiveEventContext>() {
        Effect::new(move |_| {
            if let Some(event) = context.latest_event.get()
                && should_refetch(&event)
            {
                resource.refetch();
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
            | UiEvent::AutomationChanged { .. }
    )
}

pub(crate) fn api_docs_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
            | UiEvent::CodexStatusChanged { .. }
    )
}

pub(crate) fn runs_page_event_matches(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::ProjectListChanged { .. }
            | UiEvent::ProjectChanged { .. }
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
        event_scopes_named_project, item_event_matches, run_log_event_matches,
        runs_page_event_matches, runs_section_event_matches, trigger_runs_event_matches,
    };
    use crate::shared::view_models::UiEvent;

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

        assert!(event_scopes_named_project(&event, Some(DEMO_PROJECT)));
        assert!(!event_scopes_named_project(&event, Some(OTHER_PROJECT)));
        assert!(event_scopes_named_project(&event, None));
    }

    #[test]
    fn event_scope_keeps_global_events_matching_selected_project() {
        for event in [
            project_list_changed(),
            agent_tool_changed(),
            codex_status_changed(),
        ] {
            assert!(event_scopes_named_project(&event, Some(DEMO_PROJECT)));
            assert!(event_scopes_named_project(&event, None));
        }
    }

    #[test]
    fn codex_page_refreshes_for_global_events() {
        assert!(codex_event_matches(&project_list_changed()));
        assert!(codex_event_matches(&agent_tool_changed()));
        assert!(codex_event_matches(&codex_status_changed()));
    }

    #[test]
    fn api_docs_and_runs_pages_refresh_for_shell_context_events() {
        assert!(api_docs_event_matches(&project_list_changed()));
        assert!(api_docs_event_matches(&project_changed(DEMO_PROJECT)));
        assert!(api_docs_event_matches(&codex_status_changed()));
        assert!(!api_docs_event_matches(&agent_tool_changed()));

        assert!(runs_page_event_matches(&project_list_changed()));
        assert!(runs_page_event_matches(&project_changed(DEMO_PROJECT)));
        assert!(runs_page_event_matches(&codex_status_changed()));
        assert!(!runs_page_event_matches(&agent_tool_changed()));
    }

    #[test]
    fn runs_page_shell_ignores_live_run_events() {
        assert!(!runs_page_event_matches(&automation_changed(DEMO_PROJECT)));
        assert!(!runs_page_event_matches(&agent_run_changed(
            DEMO_PROJECT,
            42,
            Some(7)
        )));
        assert!(!runs_page_event_matches(&agent_output_changed(
            DEMO_PROJECT,
            42,
            Some(7)
        )));
    }

    #[test]
    fn item_detail_refreshes_for_global_events() {
        assert!(item_event_matches(
            &project_list_changed(),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(item_event_matches(
            &agent_tool_changed(),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(item_event_matches(
            &codex_status_changed(),
            Some(DEMO_PROJECT),
            Some(7)
        ));
    }

    #[test]
    fn item_detail_matches_selected_item_events() {
        assert!(item_event_matches(
            &work_item_changed(DEMO_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(item_event_matches(
            &comment_changed(DEMO_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(item_event_matches(
            &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(item_event_matches(
            &agent_output_changed(DEMO_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
    }

    #[test]
    fn item_detail_ignores_unmatched_item_and_run_events() {
        assert!(!item_event_matches(
            &work_item_changed(DEMO_PROJECT, 8),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &comment_changed(DEMO_PROJECT, 8),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_run_changed(DEMO_PROJECT, 42, Some(8)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_output_changed(DEMO_PROJECT, 42, Some(8)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_run_changed(DEMO_PROJECT, 99, None),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_output_changed(DEMO_PROJECT, 99, None),
            Some(DEMO_PROJECT),
            Some(7)
        ));
    }

    #[test]
    fn item_detail_applies_project_scoping_before_item_matching() {
        assert!(!item_event_matches(
            &work_item_changed(OTHER_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &comment_changed(OTHER_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_run_changed(OTHER_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
        assert!(!item_event_matches(
            &agent_output_changed(OTHER_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(7)
        ));
    }

    #[test]
    fn run_log_refreshes_for_global_events() {
        assert!(run_log_event_matches(
            &project_list_changed(),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(run_log_event_matches(
            &agent_tool_changed(),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(run_log_event_matches(
            &codex_status_changed(),
            Some(DEMO_PROJECT),
            Some(42)
        ));
    }

    #[test]
    fn run_log_matches_selected_run_events() {
        assert!(run_log_event_matches(
            &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(run_log_event_matches(
            &agent_output_changed(DEMO_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
    }

    #[test]
    fn run_log_ignores_unmatched_run_events() {
        assert!(!run_log_event_matches(
            &agent_run_changed(DEMO_PROJECT, 99, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &agent_output_changed(DEMO_PROJECT, 99, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
    }

    #[test]
    fn run_log_ignores_unrelated_item_and_project_content_events() {
        assert!(!run_log_event_matches(
            &work_item_changed(DEMO_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &comment_changed(DEMO_PROJECT, 7),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &system_prompt_changed(DEMO_PROJECT),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &memory_changed(DEMO_PROJECT),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &swim_lane_changed(DEMO_PROJECT),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &work_item_state_changed(DEMO_PROJECT),
            Some(DEMO_PROJECT),
            Some(42)
        ));
    }

    #[test]
    fn run_log_applies_project_scoping_before_run_matching() {
        assert!(!run_log_event_matches(
            &agent_run_changed(OTHER_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
        assert!(!run_log_event_matches(
            &agent_output_changed(OTHER_PROJECT, 42, Some(7)),
            Some(DEMO_PROJECT),
            Some(42)
        ));
    }

    #[test]
    fn board_items_matches_board_refresh_events_for_project() {
        assert!(board_items_event_matches(
            &work_item_changed(DEMO_PROJECT, 7),
            DEMO_PROJECT
        ));
        assert!(board_items_event_matches(
            &comment_changed(DEMO_PROJECT, 7),
            DEMO_PROJECT
        ));
        assert!(board_items_event_matches(
            &agent_run_changed(DEMO_PROJECT, 42, Some(7)),
            DEMO_PROJECT
        ));
        assert!(board_items_event_matches(
            &swim_lane_changed(DEMO_PROJECT),
            DEMO_PROJECT
        ));
        assert!(board_items_event_matches(
            &work_item_state_changed(DEMO_PROJECT),
            DEMO_PROJECT
        ));
        assert!(!board_items_event_matches(
            &work_item_changed(OTHER_PROJECT, 7),
            DEMO_PROJECT
        ));
        assert!(!board_items_event_matches(
            &comment_changed(OTHER_PROJECT, 7),
            DEMO_PROJECT
        ));
        assert!(!board_items_event_matches(
            &codex_status_changed(),
            DEMO_PROJECT
        ));
    }

    #[test]
    fn run_session_sections_match_only_run_refresh_events_for_project() {
        for event in [
            automation_changed(DEMO_PROJECT),
            agent_run_changed(DEMO_PROJECT, 42, Some(7)),
            agent_output_changed(DEMO_PROJECT, 42, Some(7)),
            codex_status_changed(),
        ] {
            assert!(runs_section_event_matches(&event, DEMO_PROJECT));
            assert!(trigger_runs_event_matches(&event, DEMO_PROJECT));
        }

        for event in [
            automation_changed(OTHER_PROJECT),
            agent_run_changed(OTHER_PROJECT, 42, Some(7)),
            agent_output_changed(OTHER_PROJECT, 42, Some(7)),
            work_item_changed(DEMO_PROJECT, 7),
        ] {
            assert!(!runs_section_event_matches(&event, DEMO_PROJECT));
            assert!(!trigger_runs_event_matches(&event, DEMO_PROJECT));
        }
    }
}
