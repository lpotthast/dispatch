const PROJECT_PATH_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
pub fn spawn_path_status_checker_until(
    service: std::sync::Arc<super::service::ProjectService>,
    shutdown: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PROJECT_PATH_CHECK_INTERVAL);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(err) = service.refresh_path_statuses().await {
                        tracing::warn!(error = %format_args!("{err:#}"), "project path status check failed");
                    }
                }
            }
        }
    })
}
