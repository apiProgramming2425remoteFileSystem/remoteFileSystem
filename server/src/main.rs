use server::{config, error::ServerError, logging, run_server};

type Result<T> = std::result::Result<T, ServerError>;

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

    let listener = std::net::TcpListener::bind((config.server_host.as_str(), config.port))
        .map_err(|err| {
            ServerError::Other(anyhow::format_err!("Failed to bind to address: {}", err))
        })?;

    let server = run_server(listener, &config.filesystem_root).await?;

    server
        .await
        .map_err(|err| ServerError::Other(anyhow::format_err!("Server runtime error: {}", err)))?;

    Ok(())
}
