use crate::backend::automation::supervisor::AutomationSupervisor;
use crate::shared::page_data::*;
use rootcause::Result;
use std::{collections::BTreeMap, sync::Arc};
pub(crate) struct BoardQueryService {
    run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
    label_catalog: Arc<crate::backend::items::labels::catalog::service::CatalogService>,
    state_service: Arc<crate::backend::items::states::service::StateService>,
    lane_service: Arc<crate::backend::board::lanes::service::LaneService>,
    label_service: Arc<crate::backend::items::labels::service::LabelService>,
    item_service: Arc<crate::backend::items::service::ItemService>,
    project_service: Arc<crate::backend::projects::service::ProjectService>,
    automation_supervisor: AutomationSupervisor,
    codex_status: crate::backend::execution::codex::model::SharedCodexStatus,
    navigation: Arc<crate::backend::operator::queries::OperatorQueryService>,
}
impl BoardQueryService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
        label_catalog: Arc<crate::backend::items::labels::catalog::service::CatalogService>,
        state_service: Arc<crate::backend::items::states::service::StateService>,
        lane_service: Arc<crate::backend::board::lanes::service::LaneService>,
        label_service: Arc<crate::backend::items::labels::service::LabelService>,
        item_service: Arc<crate::backend::items::service::ItemService>,
        project_service: Arc<crate::backend::projects::service::ProjectService>,
        automation_supervisor: AutomationSupervisor,
        codex_status: crate::backend::execution::codex::model::SharedCodexStatus,
        navigation: Arc<crate::backend::operator::queries::OperatorQueryService>,
    ) -> Self {
        Self {
            run_queries,
            label_catalog,
            state_service,
            lane_service,
            label_service,
            item_service,
            project_service,
            automation_supervisor,
            codex_status,
            navigation,
        }
    }
    pub(crate) async fn page(
        &self,
        selected_project: Option<&str>,
        api_base_url: String,
    ) -> Result<BoardPage> {
        let codex_status = self.codex_status.read().await.clone();

        let projects = self.project_service.list_summaries().await?;
        let active_project_names = self.navigation.active_projects().await?;
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
            let (
                status,
                items,
                swim_lanes,
                work_item_states,
                suggestions,
                accent_colors,
                outside_state_count,
            ) = tokio::try_join!(
                self.run_queries
                    .status_for_project_id(&project.name, project.id),
                self.items(project.id),
                self.lane_service.list_by_id(project.id),
                self.state_service.list_by_id(project.id),
                self.label_service.project_labels_by_id(project.id),
                self.label_catalog.accent_colors(project.id),
                self.item_service.count_outside_states(project.id),
            )?;
            automation_running = self
                .automation_supervisor
                .is_project_running(project.id)
                .await;
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
    pub(crate) async fn items_section(&self, project: &str) -> Result<BoardItemsSection> {
        let project_id = self.project_service.id(project).await?;
        self.items_section_by_id(project_id).await
    }
    pub(crate) async fn items_section_by_id(&self, project_id: i64) -> Result<BoardItemsSection> {
        let (items, swim_lanes, work_item_states, label_accent_colors, misconfigured_item_count) =
            tokio::try_join!(
                self.items(project_id),
                self.lane_service.list_by_id(project_id),
                self.state_service.list_by_id(project_id),
                self.label_catalog.accent_colors(project_id),
                self.item_service.count_outside_states(project_id),
            )?;
        Ok(BoardItemsSection {
            items,
            swim_lanes,
            work_item_states,
            label_accent_colors,
            misconfigured_item_count,
        })
    }
    pub(crate) async fn items(&self, project_id: i64) -> Result<Vec<BoardItemView>> {
        let items = self.item_service.board(project_id).await?;
        let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
        let mut run_previews = self
            .run_queries
            .previews_for_project_id(project_id, &item_ids)
            .await?;

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
}
