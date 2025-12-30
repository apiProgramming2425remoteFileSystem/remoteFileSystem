pub mod cache;
pub mod logging;

pub use cache::*;
pub use logging::*;

use anyhow;
use clap::Parser;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ConfigError;

pub const ENV_PREFIX: &str = "RFS";
pub const ENV_SEPARATOR: &str = "__";
pub const DEFAULT_CONFIG_FILE: &str = "default_config.toml";
pub const DEFAULT_MOUNTPOINT: &str = "/mnt/remote-fs";
pub const DEFAULT_SERVER_URL: &str = "http://localhost:8080";
pub const DEFAULT_LOG_DIR: &str = "./logs";
pub const DEFAULT_LOG_FILE_NAME: &str = "remote_fs_client";
pub const DEFAULT_LOG_FILE_EXT: &str = "log";

type Result<T> = std::result::Result<T, ConfigError>;

/// App configuration
///
/// This structure holds the complete application configuration
/// including settings loaded from the configuration file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RfsConfig {
    pub mountpoint: PathBuf,
    pub server_url: String,
    pub foreground: bool,
    pub xattributes_enabled: bool,

    pub cache: CacheConfig,
    pub logging: LoggingConfig,
}

impl Default for RfsConfig {
    fn default() -> Self {
        Self {
            mountpoint: PathBuf::from(DEFAULT_MOUNTPOINT),
            server_url: DEFAULT_SERVER_URL.to_string(),
            foreground: false,
            xattributes_enabled: false,
            cache: CacheConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// CLI configuration
///
/// This structure holds the CLI arguments for the application,
/// including the path to the configuration file and other flattened modules.
#[derive(Debug, Clone, Parser, Serialize)]
// #[command(author, version, about = "Remote Filesystem Client")]
pub struct RfsCliArgs {
    /// Path to the configuration file
    #[arg(short, long, default_value = DEFAULT_CONFIG_FILE, env = "RFS_CONFIG_FILE")]
    #[serde(skip)]
    pub config_file: PathBuf,

    /// Mountpoint path (e.g /mnt/remote-fs)
    #[arg(short, long, env = "RFS_MOUNTPOINT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,

    /// Remote server base URL (e.g. http://localhost:8080/)
    #[arg(short, long, env = "RFS_SERVER_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,

    /// Run in foreground (do not daemonize).
    /// When set, the application will run in the foreground and not daemonize.
    #[arg(short, long)]
    pub foreground: bool,

    /// Enable xattributes
    #[arg(short, long)]
    pub xattributes_enabled: bool,

    /// Cache configuration
    #[command(flatten)]
    #[command(next_help_heading = "Cache Configuration")]
    pub cache: CacheCliArgs,

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
        config
            .validate()
            .map_err(|err| ConfigError::InvalidConfig(err))?;

        Ok(config)
    }
}

impl ConfigModule for RfsConfig {
    fn finalize(&mut self) {
        self.cache.finalize();
        self.logging.finalize();
    }

    fn validate(&self) -> std::result::Result<(), String> {
        self.cache
            .validate()
            .map_err(|err| format!("[Cache] {}", err))?;
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
