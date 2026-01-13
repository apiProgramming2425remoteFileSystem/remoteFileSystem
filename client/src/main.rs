use client::{config, error, network};

type Result<T> = std::result::Result<T, error::ClientError>;

fn main() -> Result<()> {
    // load configuration from args/env
    let config = config::Config::from_args()?;

    let rc = network::RemoteClient::new(&config.server_url);

    // Run the client with the provided configuration
    client::start(&config, rc)?;

    Ok(())
}
