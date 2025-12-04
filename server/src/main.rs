use server::{
    config,
    error::{self, ServerError},
    logging, run_server,
};

type Result<T> = std::result::Result<T, error::ServerError>;

#[actix_web::main]
async fn main() -> Result<()> {
    // Load configuration from args/env
    let config = config::Config::from_args()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    run_server(&config.server_host, config.port, &config.filesystem_root).await?;

    Ok(())
}
