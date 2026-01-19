use clap::{Parser, Subcommand};

use crate::commands::*;
use crate::config::*;
use crate::error::RfsClientError;

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
    // REVIEW: check if these are needed
    // /// Mount the remote filesystem
    // Mount,
    // /// Unmount the remote filesystem
    // Unmount,
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

                // Run the client with the provided configuration
                crate::start(&config)?;
            }
            Commands::TomlGen(cmd) => cmd.execute()?,
            Commands::EnvGen(cmd) => cmd.execute()?,
            // Commands::Mount => {
            //     // Implement mount logic here
            //     Ok(())
            // }
            // Commands::Unmount => {
            //     // Implement unmount logic here
            //     Ok(())
            // }
        }
        Ok(())
    }
}
