use crate::backend::{
    entities::{
        agent_run::{self, AgentRun},
        work_item_event,
        work_item_origin::{self, WorkItemOrigin},
    },
    items::events::repository as events,
    storage::Transaction,
};
use dispatch_types::WorkItemEventView;
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
pub(crate) struct PostconditionRepository;
impl PostconditionRepository {
    pub(super) async fn require_run_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<()> {
        AgentRun::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load postcondition run")?
            .ok_or_else(|| report!("agent run {run_id} does not exist in this project"))?;
        Ok(())
    }
    pub(super) async fn events_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Vec<WorkItemEventView>> {
        work_item_event::Entity::find()
            .filter(work_item_event::Column::ProjectId.eq(project_id))
            .filter(work_item_event::Column::AgentRunId.eq(run_id))
            .all(transaction.connection())
            .await
            .context("failed to load run-attributed events for postconditions")?
            .into_iter()
            .map(events::decode)
            .collect()
    }
    pub(super) async fn created_item_ids_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Vec<i64>> {
        Ok(WorkItemOrigin::find()
            .filter(work_item_origin::Column::ProjectId.eq(project_id))
            .filter(work_item_origin::Column::AgentRunId.eq(run_id))
            .all(transaction.connection())
            .await
            .context("failed to load run-attributed created items for postconditions")?
            .into_iter()
            .map(|origin| origin.work_item_id)
            .collect())
    }
}
