use std::net::{IpAddr, SocketAddr};

use leptos::prelude::get_configuration;
use rootcause::Result;

use crate::backend::{
    automation::supervisor::AutomationSupervisor, execution::sessions::ProcessSessionRegistry, http,
};

pub(crate) async fn serve(
    mut application: crate::backend::application::Application,
    bind: SocketAddr,
) -> Result<()> {
    let mut leptos_options = get_configuration(None)?.leptos_options;
    leptos_options.site_addr = bind;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    application.start_workers().await?;
    let state = application.state.clone();
    let app = http::router(state.clone(), application.contexts.clone(), leptos_options);
    tracing::info!(url = %format_args!("http://{bind}"), "Serving Dispatch");
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(
            state.runs,
            state.sessions,
            state.automation_supervisor,
            application.shutdown_sender(),
        ))
        .await;
    let workers = application.finish_workers().await;
    result?;
    workers
}

pub(crate) fn local_api_url(bind: SocketAddr) -> String {
    let host = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    format!("http://{host}:{}", bind.port())
}

async fn shutdown_signal(
    runs: std::sync::Arc<crate::backend::runs::service::RunService>,
    sessions: ProcessSessionRegistry,
    automation_supervisor: AutomationSupervisor,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
) {
    wait_for_shutdown_signal().await;
    let _ = shutdown_tx.send(true);
    automation_supervisor.shutdown_all().await;
    crate::backend::application::cancel_active_sessions(runs, &sessions).await;
}

async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = async {
            match signal(SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(err) => {
                    tracing::error!(%err, "failed to install SIGTERM handler");
                    std::future::pending::<()>().await;
                }
            }
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}
