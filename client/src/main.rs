use client::{config, error};

type Result<T> = std::result::Result<T, error::ClientError>;

fn main() -> Result<()> {
    // load configuration from args/env
    let config = config::Config::from_args()?;

    // Run the client with the provided configuration
    client::start(&config)?;

    Ok(())
}
