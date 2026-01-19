use clap::Parser;
use client::app::{Executable, RfsClient};
use client::error::RfsClientError;
use client::network;

type Result<T> = std::result::Result<T, RfsClientError>;

fn main() -> Result<()> {
    // Load .env variables
    let _ = dotenvy::dotenv();

    // let rc = network::RemoteClient::new(&config.server_url);

    let app = RfsClient::parse();
    app.command.execute()?;

    Ok(())
}
