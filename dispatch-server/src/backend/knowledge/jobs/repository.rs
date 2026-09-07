use super::model::Record;
use crate::backend::{entities::agent_run, storage::Transaction};
use dispatch_types::knowledge::jobs::{KnowledgeJob, KnowledgeSettings};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Statement, Value,
};
pub(crate) struct JobRepository;
fn statement(sql: &str, values: Vec<Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}
impl JobRepository {
    pub(crate) async fn load_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<Record> {
        let row = tx
            .connection()
            .query_one(statement(
                "SELECT payload, version FROM knowledge_jobs WHERE project_id=? AND id=?",
                vec![project_id.into(), id.into()],
            ))
            .await?
            .ok_or_else(|| report!("knowledge job does not exist in this project"))?;
        let mut record: Record = serde_json::from_str(&row.try_get::<String>("", "payload")?)?;
        if record
            .detail
            .job
            .as_ref()
            .is_none_or(|job| job.id != id || job.project_id != project_id)
        {
            bail!("knowledge job payload does not match its persisted scope");
        }
        record.version = row.try_get("", "version")?;
        Ok(record)
    }
    pub(crate) async fn persist_in(&self, tx: &Transaction, record: &Record) -> Result<()> {
        let result=tx.connection().execute(statement("UPDATE knowledge_jobs SET payload=?, unresolved=?, version=version+1 WHERE id=? AND project_id=? AND version=?",vec![serde_json::to_string(record)?.into(),record.job().status.unresolved().into(),record.job().id.into(),record.job().project_id.into(),record.version.into()])).await?;
        if result.rows_affected() != 1 {
            bail!("knowledge job changed concurrently; reload it");
        }
        Ok(())
    }
    pub(crate) async fn allocate_in(&self, tx: &Transaction, record: &Record) -> Result<i64> {
        Ok(tx
            .connection()
            .execute(statement(
                "INSERT INTO knowledge_jobs(project_id,request_id,payload) VALUES(?,?,?)",
                vec![
                    record.job().project_id.into(),
                    record.job().request.request_id.clone().into(),
                    serde_json::to_string(record)?.into(),
                ],
            ))
            .await?
            .last_insert_id() as i64)
    }
    pub(crate) async fn initialize_in(&self, tx: &Transaction, record: &Record) -> Result<()> {
        tx.connection()
            .execute(statement(
                "UPDATE knowledge_jobs SET payload=? WHERE id=? AND project_id=?",
                vec![
                    serde_json::to_string(record)?.into(),
                    record.job().id.into(),
                    record.job().project_id.into(),
                ],
            ))
            .await?;
        Ok(())
    }
    pub(crate) async fn idempotent_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        request_id: &str,
    ) -> Result<Option<Record>> {
        let row = tx
            .connection()
            .query_one(statement(
                "SELECT id FROM knowledge_jobs WHERE project_id=? AND request_id=?",
                vec![project_id.into(), request_id.into()],
            ))
            .await?;
        match row {
            Some(row) => Ok(Some(
                self.load_in(tx, project_id, row.try_get("", "id")?).await?,
            )),
            None => Ok(None),
        }
    }
    pub(crate) async fn unresolved_in(
        &self,
        tx: &Transaction,
        project_id: Option<i64>,
    ) -> Result<Vec<(i64, i64)>> {
        let (sql, args) = match project_id {
            Some(id) => (
                "SELECT id,project_id FROM knowledge_jobs WHERE project_id=? AND unresolved=1",
                vec![id.into()],
            ),
            None => (
                "SELECT id,project_id FROM knowledge_jobs WHERE unresolved=1",
                vec![],
            ),
        };
        tx.connection()
            .query_all(statement(sql, args))
            .await?
            .into_iter()
            .map(|row| Ok((row.try_get("", "project_id")?, row.try_get("", "id")?)))
            .collect()
    }
    pub(crate) async fn list_in(
        &self,
        tx: &Transaction,
        project_id: i64,
    ) -> Result<Vec<KnowledgeJob>> {
        tx.connection().query_all(statement("SELECT json_extract(payload,'$.detail.job') AS job FROM knowledge_jobs WHERE project_id=? ORDER BY id DESC",vec![project_id.into()])).await?.into_iter().map(|row|{let job:KnowledgeJob=serde_json::from_str(&row.try_get::<String>("","job")?)?;if job.project_id!=project_id {bail!("knowledge job payload does not match its persisted scope");}Ok(job)}).collect()
    }
    pub(crate) async fn settings_in(
        &self,
        tx: &Transaction,
        project_id: i64,
    ) -> Result<KnowledgeSettings> {
        match tx
            .connection()
            .query_one(statement(
                "SELECT payload FROM knowledge_job_settings WHERE project_id=?",
                vec![project_id.into()],
            ))
            .await?
        {
            Some(row) => Ok(serde_json::from_str(
                &row.try_get::<String>("", "payload")?,
            )?),
            None => Ok(KnowledgeSettings::default()),
        }
    }
    pub(crate) async fn save_settings_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        settings: &KnowledgeSettings,
    ) -> Result<()> {
        tx.connection().execute(statement("INSERT INTO knowledge_job_settings(project_id,payload) VALUES(?,?) ON CONFLICT(project_id) DO UPDATE SET payload=excluded.payload",vec![project_id.into(),serde_json::to_string(settings)?.into()])).await?;
        Ok(())
    }
    pub(crate) async fn reader_job_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<i64>> {
        Ok(agent_run::Entity::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(tx.connection())
            .await?
            .and_then(|run| run.knowledge_job_id))
    }
    pub(crate) async fn linked_running_runs_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        job_id: i64,
    ) -> Result<Vec<i64>> {
        Ok(agent_run::Entity::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::KnowledgeJobId.eq(job_id))
            .filter(agent_run::Column::Status.eq("running"))
            .all(tx.connection())
            .await?
            .into_iter()
            .map(|run| run.id)
            .collect())
    }
    pub(crate) async fn run_tokens_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        job_id: i64,
    ) -> Result<u64> {
        Ok(agent_run::Entity::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::KnowledgeJobId.eq(job_id))
            .all(tx.connection())
            .await?
            .iter()
            .map(|run| {
                run.input_tokens
                    .unwrap_or(0)
                    .saturating_add(run.output_tokens.unwrap_or(0))
                    .max(0) as u64
            })
            .sum())
    }
}
