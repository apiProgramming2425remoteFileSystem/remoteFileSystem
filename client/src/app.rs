use clap::{Parser, Subcommand};

use crate::commands::*;
use crate::config::*;
use crate::error::RfsClientError;
use crate::network::*;

pub fn run() -> Result<(), RfsClientError> {
    // Load .env variables
    let _ = dotenvy::dotenv();

    let app = RfsClient::parse();

    app.command.execute()
}

/// Application CLI
#[derive(Debug, Clone, Parser)]
#[command(author, version, about = "Remote Filesystem Client")]
pub struct RfsClient {
    /// Subcommand to execute
    #[command(subcommand, next_help_heading = "Commands")]
    pub command: Commands,
}

/// CLI subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Start the remote filesystem client
    Run(Box<RfsCliArgs>),
    /// Generate a default configuration file
    TomlGen(TomlConfigGenerator),
    /// Generate environment variable template
    EnvGen(EnvVarGenerator),
    /// Unmount the remote filesystem
    Unmount(CliUnmountArgs),
}

/// Trait for CLI commands that can be executed.
pub trait Executable {
    type Error;

    fn execute(&self) -> Result<(), Self::Error>;
}

impl Executable for Commands {
    type Error = RfsClientError;

    /// Execute the selected subcommand.
    fn execute(&self) -> Result<(), Self::Error> {
        match &self {
            Commands::Run(cmd) => {
                // Load configuration from args/env
                let config = RfsConfig::load(cmd)?;

                let rc = RemoteClient::new(&config.server_url);

                // Run the client with the provided configuration
                crate::start(&config, rc)?;
            }
            Commands::TomlGen(cmd) => cmd.execute()?,
            Commands::EnvGen(cmd) => cmd.execute()?,
            Commands::Unmount(cmd) => cmd.execute()?,
        }
        Ok(())
    }
}
