pub mod logging;
pub use logging::*;

use crate::error::ConfigError;

use anyhow;
use clap::Parser;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const ENV_PREFIX: &str = "RFS";
pub const ENV_SEPARATOR: &str = "__";
pub const DEFAULT_DATABASE_PATH: &str = "database/db.sqlite";
pub const DEFAULT_CONFIG_FILE: &str = "server_config.toml";
pub const DEFAULT_SERVER_HOST: &str = "localhost";
pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_FILESYSTEM_ROOT: &str = "/remote_fs";
pub const DEFAULT_LOG_DIR: &str = "./logs";
pub const DEFAULT_LOG_FILE_NAME: &str = "remote_fs_server";
pub const DEFAULT_LOG_FILE_EXT: &str = "log";
pub const DEFAULT_LOG_FILE_ROT: &str = "never";

type Result<T> = std::result::Result<T, ConfigError>;

/// App configuration
///
/// This structure holds the complete application configuration
/// including settings loaded from the configuration file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RfsConfig {
    /// Server hostname or IP to bind to
    pub server_host: String,
    /// Server port to listen on
    pub server_port: u16,
    /// Root directory for the remote filesystem
    pub filesystem_root: PathBuf,
    /// Logging configuration
    pub logging: LoggingConfig,
}

impl Default for RfsConfig {
    fn default() -> Self {
        Self {
            server_host: DEFAULT_SERVER_HOST.to_string(),
            server_port: DEFAULT_PORT,
            filesystem_root: PathBuf::from(DEFAULT_FILESYSTEM_ROOT),
            logging: LoggingConfig::default(),
        }
    }
}

/// CLI configuration
///
/// This structure holds the CLI arguments for the application,
/// including the path to the configuration file and other flattened modules.
#[derive(Debug, Clone, Parser, Serialize)]
#[command(author, version, about = "Remote Filesystem Server")]
pub struct RfsCliArgs {
    /// Path to the configuration file
    #[arg(short, long, default_value = DEFAULT_CONFIG_FILE, env = "RFS__CONFIG_FILE")]
    #[serde(skip)]
    pub config_file: PathBuf,

    /// Server hostname or IP to bind to
    #[arg(short, long, env = "RFS__SERVER_HOST")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_host: Option<String>,

    /// Server port to listen on
    #[arg(short = 'p', long, env = "RFS__SERVER_PORT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_port: Option<u16>,

    /// Root directory for the remote filesystem
    #[arg(short, long, env = "RFS__FILESYSTEM_ROOT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem_root: Option<PathBuf>,

    /// Logging configuration
    #[command(flatten)]
    #[command(next_help_heading = "Logging Configuration")]
    pub logging: LoggingCliArgs,
}

pub trait ConfigModule {
    /// Post-processing: compute conditional defaults, normalize paths, etc.
    /// Called AFTER merging all sources.
    fn finalize(&mut self) {
        // Default: do nothing
    }

    /// Validation: check that values are consistent.
    /// Returns a simple error string. The caller will wrap it in a specific error.
    fn validate(&self) -> std::result::Result<(), String> {
        // Default: All good
        Ok(())
    }
}

pub trait Formatter {
    fn format<T: Serialize>(&self, value: &T) -> std::result::Result<String, String>;
}

impl RfsConfig {
    /// Load configuration from the specified file path, environment variables, and CLI arguments.
    ///
    /// The order of precedence (highest to lowest) is:
    /// 1. CLI arguments
    /// 2. Environment variables (with prefix [`ENV_PREFIX`] and separator [`ENV_SEPARATOR`])
    /// 3. Configuration file
    /// 4. Default values
    ///
    /// Returns the loaded configuration or an error.
    pub fn load(args: &RfsCliArgs) -> Result<Self> {
        // Build the configuration by merging sources
        let builder = Config::builder()
            // Load default values from the modules to pass to the builder
            .add_source(Config::try_from(&RfsConfig::default()).map_err(|err| {
                ConfigError::Other(anyhow::format_err!(
                    "Failed to convert default config: {}",
                    err
                ))
            })?)
            // Load configuration from the specified file
            .add_source(File::from(args.config_file.as_path()).required(false))
            // Load environment variables (with prefix "RFS" to limit scope)
            .add_source(Environment::with_prefix(ENV_PREFIX).separator(ENV_SEPARATOR))
            // Override with CLI arguments, done automatically via Serialize
            .add_source(Config::try_from(&args).map_err(|err| {
                ConfigError::Other(anyhow::format_err!("Failed to convert CLI args: {}", err))
            })?);

        let mut config: RfsConfig = builder
            .build()
            .map_err(|err| {
                ConfigError::Other(anyhow::format_err!(
                    "Failed to build configuration: {}",
                    err
                ))
            })?
            .try_deserialize()
            .map_err(|err| {
                ConfigError::InvalidConfig(format!("Failed to deserialize configuration: {}", err))
            })?;

        // Post-process: finalize and validate
        config.finalize();
        config.validate().map_err(ConfigError::InvalidConfig)?;

        Ok(config)
    }
}

impl ConfigModule for RfsConfig {
    fn finalize(&mut self) {
        self.logging.finalize();
    }

    fn validate(&self) -> std::result::Result<(), String> {
        self.logging
            .validate()
            .map_err(|err| format!("[Logging] {}", err))?;
        Ok(())
    }
}

pub struct TomlFormatter;

impl Formatter for TomlFormatter {
    fn format<T: Serialize>(&self, value: &T) -> std::result::Result<String, String> {
        toml::to_string_pretty(value).map_err(|err| format!("Failed to serialize to TOML: {}", err))
    }
}
