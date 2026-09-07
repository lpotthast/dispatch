use super::service::JobService;
use dispatch_types::knowledge::jobs::JobStatus;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
pub(crate) struct JobWorker {
    service: Arc<JobService>,
}
impl JobWorker {
    pub(crate) fn new(service: Arc<JobService>) -> Self {
        Self { service }
    }
    pub(crate) fn spawn_until(self, shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let active = Arc::new(Mutex::new(HashSet::new()));
            let mut jobs = tokio::task::JoinSet::new();
            loop {
                if shutdown.is_cancelled() {
                    break;
                }
                if let Ok(ids) = self.service.unresolved().await {
                    for (project_id, id) in ids {
                        let Ok(record) = self.service.load(project_id, id).await else {
                            continue;
                        };
                        if !matches!(record.job().status, JobStatus::Queued | JobStatus::Running)
                            || !active.lock().await.insert(id)
                        {
                            continue;
                        }
                        let (service, shutdown, active) =
                            (self.service.clone(), shutdown.child_token(), active.clone());
                        jobs.spawn(async move{if let Err(error)=service.drive(shutdown,project_id,id).await {tracing::warn!(job_id=id,"Knowledge execution stopped: {error}");if let Err(recovery)=service.fail(project_id,id,&error.to_string()).await {tracing::warn!(job_id=id,error=%recovery,"Knowledge failure recovery stopped");}}active.lock().await.remove(&id);});
                    }
                }
                while let Some(result) = jobs.try_join_next() {
                    if let Err(error) = result {
                        tracing::error!(%error,"knowledge worker failed");
                    }
                }
                tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=shutdown.cancelled()=>{}}
            }
            while let Some(result) = jobs.join_next().await {
                if let Err(error) = result {
                    tracing::error!(%error,"knowledge worker failed during shutdown");
                }
            }
        })
    }
}
