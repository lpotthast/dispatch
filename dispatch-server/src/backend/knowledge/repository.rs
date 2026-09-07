use crate::backend::{entities::agent_run, storage::Transaction};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
pub(crate) struct KnowledgeRepository;
impl KnowledgeRepository {
    pub(crate) async fn working_directory_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<String> {
        let run = agent_run::Entity::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(tx.connection())
            .await?
            .ok_or_else(|| report!("agent run is missing"))?;
        if run.working_dir.is_empty() {
            bail!("agent run has no registered working copy in this project");
        }
        Ok(run.working_dir)
    }
}
