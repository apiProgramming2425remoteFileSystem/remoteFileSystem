use async_trait::async_trait;
use clap::{Parser, Subcommand};

use crate::commands::*;
use crate::config::*;
use crate::db::DB;
use crate::error::RfsServerError;
use crate::logging::Logging;

pub async fn run() -> Result<(), RfsServerError> {
    // Load .env variables
    let _ = dotenvy::dotenv();

    let app = RfsServer::parse();

    // Initialize database connection
    let db_conn = DB::open_connection(&app.database_path).await?;

    // Execute the selected subcommand.
    app.command.execute(db_conn).await
}

/// Application CLI
#[derive(Debug, Clone, Parser)]
#[command(author, version, about = "Remote Filesystem Server")]
pub struct RfsServer {
    /// Subcommand to execute
    #[command(subcommand, next_help_heading = "Commands")]
    pub command: Commands,

    /// Path to the database file
    #[arg(
        short,
        long,
        env = "DATABASE_PATH",
        default_value = DEFAULT_DATABASE_PATH
    )]
    pub database_path: String,
}

/// CLI subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// Start the remote filesystem client
    Run(RfsCliArgs),
    /// Create user
    UserCreate(UserCreateCommand),
    /// Change username
    UserChangeUsername(UserChangeUsernameCommand),
    /// Change user password
    UserChangePassword(UserChangePasswordCommand),
    /// Delete user
    UserDelete(UserDeleteCommand),
    // REVIEW: check if these are needed
    /*
    /// List users
    UserList(UserListCommand),
    /// Modify user permissions
    UserModifyPermissions(UserModifyPermissionsCommand),
    /// List user permissions
    UserListPermissions(UserListPermissionsCommand),
    /// Show user details
    UserShow(UserShowCommand),
    /// Reset user password
    UserResetPassword(UserResetPasswordCommand),
    */
    /// Generate a default configuration file
    TomlGen(TomlConfigGenerator),
    /// Generate environment variable template
    EnvGen(EnvVarGenerator),
}

/// Trait for CLI commands that can be executed.
#[async_trait]
pub trait Executable {
    type Error;

    fn execute(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn execute_with_db(&self, _db: DB) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Commands {
    /// Execute the selected command.
    pub async fn execute(&self, db: DB) -> Result<(), RfsServerError> {
        match &self {
            Commands::Run(cmd) => {
                // Load configuration from args/env
                let config = RfsConfig::load(cmd)?;

                // Initialize logging based on config
                let _log = Logging::from(&config.logging)?;

                tracing::trace!("[TRACE]");
                tracing::debug!("[DEBUG]");
                tracing::info!("[INFO]");
                tracing::warn!("[WARN]");
                tracing::error!("[ERROR]");

                crate::run_server(
                    &config.server_host,
                    config.server_port,
                    &config.filesystem_root,
                    db,
                )
                .await?;
            }
            Commands::UserCreate(cmd) => cmd.execute_with_db(db).await?,
            Commands::UserChangeUsername(cmd) => cmd.execute_with_db(db).await?,
            Commands::UserChangePassword(cmd) => cmd.execute_with_db(db).await?,
            Commands::UserDelete(cmd) => cmd.execute_with_db(db).await?,
            Commands::TomlGen(cmd) => cmd.execute()?,
            Commands::EnvGen(cmd) => cmd.execute()?,
        }
        Ok(())
    }
}
