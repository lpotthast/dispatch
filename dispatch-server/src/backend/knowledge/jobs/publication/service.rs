use crate::backend::knowledge::jobs::{model::Record, service::JobService};
use dispatch_types::knowledge::jobs::*;
use rootcause::{Result, prelude::*};
impl JobService {
    pub(in crate::backend::knowledge::jobs) async fn apply(
        &self,
        project: &str,
        id: i64,
    ) -> Result<KnowledgeJob> {
        let _admission = self.admission.acquire().await;
        let lock = self.coordination.lock().await;
        let tx = self.transactions.begin().await?;
        let project = self.projects.by_name_in(&tx, project).await?;
        let project_id = project.id;
        let mut record = self.repository.load_in(&tx, project_id, id).await?;
        if record.job().status == JobStatus::Completed
            && record.job().outcome.starts_with("Applied")
        {
            tx.commit().await?;
            return Ok(record.job().clone());
        }
        if record.job().stage != JobStage::Publication
            && (record.job().status != JobStatus::AwaitingReview || record.job().cancel_requested)
        {
            bail!("only an active pending proposal can be applied");
        }
        if self
            .admission
            .running_counts_in(&tx, project_id)
            .await?
            .mutating
            > 0
        {
            bail!("publication is waiting for workspace writers");
        }
        if project.path.as_deref() != Some(&record.workspace)
            || project.knowledge_directory != record.directory
        {
            bail!("project working copy or knowledge directory changed; refresh this proposal");
        }
        tx.commit().await?;
        if record.job().stage == JobStage::Publication {
            self.recover_publication(&mut record).await?;
            return Ok(record.job().clone());
        }
        self.files.check_fresh(record.clone()).await?;
        let checked = self.files.candidate(record.clone()).await?;
        if !checked.detail.validation.is_empty() {
            bail!(
                "candidate has structural or propagation defects: {}",
                checked.detail.validation.join("; ")
            );
        }
        if checked.detail.changes != record.detail.changes {
            bail!("candidate changed after review; refresh the job before applying");
        }
        let tx = self.transactions.begin().await?;
        let current = self.projects.by_id_in(&tx, project_id).await?;
        if current.path != project.path
            || current.knowledge_directory != project.knowledge_directory
        {
            bail!("project working copy or knowledge directory changed; refresh this proposal");
        }
        record.job_mut().stage = JobStage::Publication;
        self.persist_in(&tx, &mut record).await?;
        tx.commit().await?;
        drop(lock);
        let result = self.files.publish(record).await;
        let _lock = self.coordination.lock().await;
        let mut record = self.load(project_id, id).await?;
        match result {
            Ok(()) => {
                record.job_mut().status = JobStatus::Completed;
                record.job_mut().stage = JobStage::Finished;
                record.job_mut().outcome = if record.job().cancel_requested {
                    "Applied changes; cancellation arrived during publication"
                } else {
                    "Applied changes"
                }
                .into();
            }
            Err(error) => {
                record.job_mut().status = JobStatus::AwaitingReview;
                record.job_mut().outcome = format!("Publication requires recovery: {error}");
            }
        }
        self.persist(&mut record).await?;
        Ok(record.job().clone())
    }
    pub(in crate::backend::knowledge::jobs) async fn recover_publication(
        &self,
        record: &mut Record,
    ) -> Result<()> {
        match self.files.publish(record.clone()).await {
            Ok(()) => {
                record.job_mut().status = JobStatus::Completed;
                record.job_mut().stage = JobStage::Finished;
                record.job_mut().outcome = "Applied changes; publication recovered".into();
            }
            Err(error) => {
                record.job_mut().status = JobStatus::AwaitingReview;
                record.job_mut().outcome = format!("Publication conflict preserved: {error}");
            }
        }
        self.persist(record).await
    }
}
