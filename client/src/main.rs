use anyhow;
use client::{config, logging, network};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration from args/env
    let config = config::Config::from_args()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    println!("Done");

    let base_url = config.server_url + network::APP_V1_BASE_URL;
    let rc = network::client::RemoteClient::new(base_url);
    rc.list_path("/").await?;

    // Start daemon and mount FUSE file system
    // daemon::run_daemon(config).await?;

    Ok(())
}
