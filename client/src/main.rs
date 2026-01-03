use clap::Parser;
use client::app::{Executable, RfsClient};
use client::error::RfsClientError;

type Result<T> = std::result::Result<T, RfsClientError>;

fn main() -> Result<()> {
    // Load .env variables
    let _ = dotenvy::dotenv();

    let app = RfsClient::parse();
    app.command.execute()?;

    Ok(())
}
