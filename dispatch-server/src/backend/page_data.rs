use std::collections::{BTreeMap, HashSet};

use rootcause::Result;

use crate::{
    backend::{
        automation, automation_admission, automation_controller::AutomationController, comments,
        item_label_service, items, label_keys, personalities,
        process_sessions::ProcessSessionRegistry, projects, relationships, storage::Store,
        swim_lanes, work_item_states, workspace,
    },
    frontend::{
        ApiDocsPage, BoardItemView, BoardItemsSection, BoardPage, BoardRunPreview, CodexStatusPage,
        ItemPage, MetricsPageData, ProjectPage, ProjectsPage, RunLogPage, RunSummaryView,
        RunsSection, TriggersPage, WorkspaceBarData,
    },
    shared::view_models::{AgentRunView, CodexAppServerStatusView},
};

pub(crate) async fn board_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
    api_base_url: String,
) -> Result<BoardPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);

    let selected_project_view = selected_project
        .as_deref()
        .and_then(|project| projects.iter().find(|candidate| candidate.name == project))
        .cloned();

    let mut automation_status = None;
    let mut automation_running = false;
    let mut project_items = Vec::new();
    let mut project_swim_lanes = Vec::new();
    let mut project_work_item_states = Vec::new();
    let mut label_suggestions = Vec::new();
    let mut label_accent_colors = BTreeMap::new();
    let mut misconfigured_item_count = 0;
    if let Some(project) = selected_project_view.as_ref() {
        let db = store.db();
        let (
            status,
            items,
            swim_lanes,
            work_item_states,
            suggestions,
            accent_colors,
            outside_state_count,
        ) = tokio::try_join!(
            automation::automation_status_for_project_id(store, &project.name, project.id),
            board_items(store, project.id),
            swim_lanes::list_swim_lanes_for_project_id(store, project.id),
            work_item_states::list_work_item_states_for_project_id(store, project.id),
            item_label_service::list_project_labels_for_project_id(store, project.id),
            label_keys::accent_colors_for_project(db.as_ref(), project.id),
            items::count_items_outside_work_item_states_for_project_id(store, project.id),
        )?;
        automation_running = automation_controller.is_project_running(project.id).await;
        automation_status = Some(status);
        project_items = items;
        project_swim_lanes = swim_lanes;
        project_work_item_states = work_item_states;
        label_suggestions = suggestions;
        label_accent_colors = accent_colors;
        misconfigured_item_count = outside_state_count;
    }

    Ok(BoardPage {
        projects,
        active_project_names,
        selected_project,
        selected_project_view,
        automation_status,
        automation_running,
        items: project_items,
        swim_lanes: project_swim_lanes,
        work_item_states: project_work_item_states,
        label_suggestions,
        label_accent_colors,
        misconfigured_item_count,
        api_base_url,
        codex_status,
    })
}

pub(crate) async fn board_items_section(store: &Store, project: &str) -> Result<BoardItemsSection> {
    let project_id = projects::project_id(store, project).await?;
    board_items_section_for_project_id(store, project_id).await
}

async fn board_items_section_for_project_id(
    store: &Store,
    project_id: i64,
) -> Result<BoardItemsSection> {
    let db = store.db();
    let (items, swim_lanes, work_item_states, label_accent_colors, misconfigured_item_count) = tokio::try_join!(
        board_items(store, project_id),
        swim_lanes::list_swim_lanes_for_project_id(store, project_id),
        work_item_states::list_work_item_states_for_project_id(store, project_id),
        label_keys::accent_colors_for_project(db.as_ref(), project_id),
        items::count_items_outside_work_item_states_for_project_id(store, project_id),
    )?;
    Ok(BoardItemsSection {
        items,
        swim_lanes,
        work_item_states,
        label_accent_colors,
        misconfigured_item_count,
    })
}

async fn board_items(store: &Store, project_id: i64) -> Result<Vec<BoardItemView>> {
    let items = items::list_board_items_for_project_id(store, project_id).await?;
    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let mut run_previews =
        automation::list_item_run_previews_for_project_id(store, project_id, &item_ids).await?;

    Ok(items
        .into_iter()
        .map(|item| {
            let previews = run_previews.remove(&item.id).unwrap_or_default();
            BoardItemView {
                item,
                run_count: previews.total,
                recent_runs: previews
                    .latest
                    .into_iter()
                    .map(|run| BoardRunPreview {
                        id: run.id,
                        status: run.status,
                        result_summary: run.result_summary,
                        created_at: run.created_at,
                    })
                    .collect(),
            }
        })
        .collect())
}

pub(crate) async fn runs_section(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    automation_controller: &AutomationController,
    project: &str,
) -> Result<RunsSection> {
    let project_id = projects::project_id(store, project).await?;
    let (running, recent_runs) = tokio::try_join!(
        automation_admission::running_counts_for_project_id(store, project_id),
        automation::list_runs_for_project_id(store, project_id, Some(10)),
    )?;
    let automation_running = automation_controller.is_project_running(project_id).await;
    let active_run_ids = sessions.active_run_ids_for_project(project_id);
    let runs = run_summaries(store, project, recent_runs, active_run_ids).await?;

    Ok(RunsSection {
        automation_running,
        running_runs: running.total(),
        running_mutating_runs: running.mutating,
        running_read_only_runs: running.read_only,
        runs,
    })
}

async fn run_summaries(
    store: &Store,
    project: &str,
    recent_runs: Vec<AgentRunView>,
    active_run_ids: Vec<i64>,
) -> Result<Vec<RunSummaryView>> {
    let active_ids = active_run_ids.iter().copied().collect::<HashSet<_>>();
    let mut ordered = Vec::new();
    let mut seen = HashSet::new();

    for run in recent_runs
        .iter()
        .filter(|run| active_ids.contains(&run.id))
        .cloned()
    {
        seen.insert(run.id);
        ordered.push(run);
    }
    let missing_active_ids = active_run_ids
        .into_iter()
        .filter(|run_id| !seen.contains(run_id))
        .collect::<Vec<_>>();
    for run_id in missing_active_ids {
        let run = automation::get_run(store, project, run_id).await?;
        seen.insert(run.id);
        ordered.push(run);
    }
    for run in recent_runs
        .into_iter()
        .filter(|run| !seen.contains(&run.id))
    {
        ordered.push(run);
    }

    Ok(ordered
        .into_iter()
        .map(|run| RunSummaryView {
            active: active_ids.contains(&run.id),
            run,
        })
        .collect())
}

pub(crate) async fn trigger_run_summaries(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    project: &str,
    trigger_id: i64,
) -> Result<Vec<RunSummaryView>> {
    let project_id = projects::project_id(store, project).await?;
    let runs = automation::list_runs_for_trigger(store, project, trigger_id, None).await?;
    let run_ids = runs.iter().map(|run| run.id).collect::<HashSet<_>>();
    let active_run_ids = sessions
        .active_run_ids_for_project(project_id)
        .into_iter()
        .filter(|run_id| run_ids.contains(run_id))
        .collect::<Vec<_>>();
    run_summaries(store, project, runs, active_run_ids).await
}

pub(crate) async fn item_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    project: &str,
    item_id: i64,
    api_base_url: String,
    codex_status: CodexAppServerStatusView,
) -> Result<ItemPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let item = items::get_item(store, project, item_id).await?;
    let comments = comments::list_comments(store, project, item_id).await?;
    let relationships = relationships::list_item_relationships(store, project, item_id).await?;
    let label_suggestions = item_label_service::list_project_labels(store, project).await?;
    let work_item_states = work_item_states::list_work_item_states(store, project).await?;
    let automation_runs = automation::list_runs_for_item(store, project, item_id, Some(10)).await?;
    Ok(ItemPage {
        projects,
        active_project_names,
        project: project.to_owned(),
        item,
        comments,
        relationships,
        label_suggestions,
        work_item_states,
        automation_runs,
        api_base_url,
        codex_status,
    })
}

pub(crate) async fn run_log_page_data(
    store: &Store,
    sessions: &ProcessSessionRegistry,
    automation_controller: &AutomationController,
    project: &str,
    run_id: i64,
    codex_status: CodexAppServerStatusView,
) -> Result<RunLogPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let run_log =
        automation::read_run_log_with_active_session(store, sessions, project, run_id).await?;
    Ok(RunLogPage {
        projects,
        active_project_names,
        project: project.to_owned(),
        run_log,
        codex_status,
    })
}

pub(crate) async fn projects_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
) -> Result<ProjectsPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;

    Ok(ProjectsPage {
        projects,
        active_project_names,
        codex_status,
    })
}

pub(crate) async fn workspace_bar_data(
    store: &Store,
    selected_project: Option<&str>,
) -> Result<WorkspaceBarData> {
    let project = match selected_project {
        Some(project) => Some(projects::get_project(store, project).await?),
        None => None,
    };

    Ok(WorkspaceBarData {
        project,
        workspace_editors: workspace::available_workspace_editors(),
    })
}

pub(crate) async fn project_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
    api_base_url: String,
) -> Result<ProjectPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);
    let selected_project_view = selected_project
        .as_deref()
        .and_then(|project| projects.iter().find(|candidate| candidate.name == project))
        .cloned();
    let system_prompt_events = if let Some(project) = selected_project.as_deref() {
        projects::list_system_prompt_events(store, project).await?
    } else {
        Vec::new()
    };

    Ok(ProjectPage {
        projects,
        active_project_names,
        selected_project,
        selected_project_view,
        system_prompt_events,
        api_base_url,
        codex_status,
    })
}

pub(crate) async fn triggers_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
    api_base_url: String,
) -> Result<TriggersPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);
    let selected_project_view = selected_project
        .as_deref()
        .and_then(|project| projects.iter().find(|candidate| candidate.name == project))
        .cloned();
    let project_personalities = if let Some(project) = selected_project_view
        .as_ref()
        .map(|project| project.name.as_str())
    {
        personalities::list_personalities(store, project).await?
    } else {
        Vec::new()
    };
    let settings = if let Some(project) = selected_project.as_deref() {
        Some(projects::get_settings(store, project).await?)
    } else {
        None
    };

    Ok(TriggersPage {
        projects,
        active_project_names,
        selected_project,
        selected_project_view,
        settings,
        personalities: project_personalities,
        api_base_url,
        codex_status,
    })
}

pub(crate) async fn codex_status_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
) -> Result<CodexStatusPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);

    Ok(CodexStatusPage {
        projects,
        active_project_names,
        selected_project,
        codex_status,
    })
}

pub(crate) async fn metrics_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
) -> Result<MetricsPageData> {
    let (projects, active_project_names) = tokio::try_join!(
        projects::list_project_summaries(store),
        active_project_names(store, automation_controller),
    )?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);

    Ok(MetricsPageData {
        projects,
        active_project_names,
        selected_project,
        codex_status,
        metrics: crate::backend::metrics::snapshot(),
    })
}

pub(crate) async fn api_docs_page_data(
    store: &Store,
    automation_controller: &AutomationController,
    codex_status: CodexAppServerStatusView,
    selected_project: Option<&str>,
) -> Result<ApiDocsPage> {
    let projects = projects::list_project_summaries(store).await?;
    let active_project_names = active_project_names(store, automation_controller).await?;
    let selected_project = selected_project
        .filter(|selected| projects.iter().any(|project| project.name == *selected))
        .map(ToOwned::to_owned);

    Ok(ApiDocsPage {
        projects,
        active_project_names,
        selected_project,
        codex_status,
    })
}

async fn active_project_names(
    store: &Store,
    automation_controller: &AutomationController,
) -> Result<Vec<String>> {
    let mut active = automation::active_project_names(store).await?;
    active.extend(automation_controller.active_project_names().await);
    active.sort();
    active.dedup();
    Ok(active)
}
