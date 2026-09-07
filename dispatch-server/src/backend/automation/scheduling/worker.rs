use super::service::SchedulerService;
use crate::backend::{
    automation::supervisor::AutomationSupervisor, items::claims::service::ClaimService,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
pub(crate) struct SchedulerWorker {
    scheduler: Arc<SchedulerService>,
    claims: Arc<ClaimService>,
    supervisor: AutomationSupervisor,
}
impl SchedulerWorker {
    pub(crate) fn new(
        scheduler: Arc<SchedulerService>,
        claims: Arc<ClaimService>,
        supervisor: AutomationSupervisor,
    ) -> Self {
        Self {
            scheduler,
            claims,
            supervisor,
        }
    }
    pub(crate) fn spawn_until(
        self,
        mut shutdown: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut automation_interval = tokio::time::interval(Duration::from_secs(1));
            let mut maintenance_interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                if *shutdown.borrow() {
                    break;
                }
                tokio::select! {
                _=automation_interval.tick()=>{let cancellations=self.supervisor.project_cancellations().await;if !cancellations.is_empty(){let ids=cancellations.keys().copied().collect::<Vec<_>>();if let Err(error)=self.scheduler.run_due(Some(&ids),Some(&cancellations)).await{tracing::error!(error=%format_args!("{error:#}"),"automation trigger scheduler failed");}}},
                _=maintenance_interval.tick()=>{if let Err(error)=self.claims.recover_all_configured().await{tracing::error!(error=%format_args!("{error:#}"),"stale claim recovery failed");}},
                changed=shutdown.changed()=>if changed.is_err()||*shutdown.borrow(){break;}
                }
            }
        })
    }
}
