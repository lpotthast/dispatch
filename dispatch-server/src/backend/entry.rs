use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use rootcause::Result;

use crate::backend::{application::Application, metrics, server, storage::default_database_path};

#[derive(Debug, Parser)]
#[command(name = "dispatch-server")]
#[command(about = "Dispatch server")]
struct ServerArgs {
    #[arg(long, env = "DISPATCH_DATABASE")]
    database: Option<PathBuf>,

    #[arg(long, default_value_t = default_bind_addr())]
    bind: SocketAddr,
}

fn default_bind_addr() -> SocketAddr {
    std::env::var("LEPTOS_SITE_ADDR")
        .or_else(|_| std::env::var("DISPATCH_BIND"))
        .unwrap_or_else(|_| "127.0.0.1:4000".to_owned())
        .parse()
        .expect("LEPTOS_SITE_ADDR or DISPATCH_BIND must be a valid socket address")
}

pub async fn run() -> Result<()> {
    crate::tracing_init::init();
    metrics::install()?;

    let args = ServerArgs::parse();
    let database = args.database.unwrap_or_else(default_database_path);
    tracing::info!(path = %database.display(), "Database path");
    let application = Application::open(database, server::local_api_url(args.bind)).await?;

    tracing::info!(url = %format_args!("http://{}", args.bind), "Starting Dispatch");
    server::serve(application, args.bind).await
}
