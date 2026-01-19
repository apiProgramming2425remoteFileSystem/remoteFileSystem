pub mod cache;
pub mod file_system;
pub mod logging;
pub mod mount;

pub use cache::*;
pub use file_system::*;
pub use logging::*;
pub use mount::*;

use anyhow;
use clap::Parser;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ConfigError;

pub const ENV_PREFIX: &str = "RFS";
pub const ENV_SEPARATOR: &str = "__";
pub const DEFAULT_CONFIG_FILE: &str = "client_config.toml";
pub const DEFAULT_MOUNTPOINT: &str = "/mnt/remote-fs";
pub const DEFAULT_SERVER_URL: &str = "http://localhost:8080";
pub const DEFAULT_PAGE_SIZE: usize = 4096;
pub const DEFAULT_MAX_PAGES: usize = 256;
pub const DEFAULT_BUFFER_SIZE: usize = 2 * 1024 * 1024;
pub const DEFAULT_CACHE_TTL: u64 = 300; // in seconds
pub const DEFAULT_CACHE_MAX_SIZE: usize = 1_048_576; // 1 MB
pub const DEFAULT_CACHE_CAPACITY: usize = 50;
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
    /// Path to the configuration file
    pub mountpoint: PathBuf,
    /// Remote server base URL
    pub server_url: String,
    /// Run in foreground without daemonizing
    pub foreground: bool,
    /// Mount configuration
    pub mount: MountConfig,
    /// Filesystem configuration
    pub file_system: FsConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// GUI availability
    pub no_gui: bool,
}

impl Default for RfsConfig {
    fn default() -> Self {
        Self {
            mountpoint: PathBuf::from(DEFAULT_MOUNTPOINT),
            server_url: DEFAULT_SERVER_URL.to_string(),
            foreground: false,
            mount: MountConfig::default(),
            file_system: FsConfig::default(),
            cache: CacheConfig::default(),
            logging: LoggingConfig::default(),
            no_gui: false,
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
    #[arg(short, long, default_value = DEFAULT_CONFIG_FILE, env = "RFS__CONFIG_FILE")]
    #[serde(skip)]
    pub config_file: PathBuf,

    /// Mountpoint path (e.g /mnt/remote-fs)
    #[arg(short, long, env = "RFS__MOUNTPOINT")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<PathBuf>,

    /// Remote server base URL (e.g. http://localhost:8080/)
    #[arg(short, long, env = "RFS__SERVER_URL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,

    /// Run the application in the foreground without daemonizing.
    #[arg(short, long, num_args = 0, default_missing_value = "false")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<bool>,

    /// Mount configuration
    #[command(flatten)]
    #[command(next_help_heading = "Mount Configuration")]
    pub mount: MountCliArgs,

    /// Filesystem configuration
    #[command(flatten)]
    #[command(next_help_heading = "Filesystem Configuration")]
    pub file_system: FsCliArgs,

    /// Cache configuration
    #[command(flatten)]
    #[command(next_help_heading = "Cache Configuration")]
    pub cache: CacheCliArgs,

    /// Logging configuration
    #[command(flatten)]
    #[command(next_help_heading = "Logging Configuration")]
    pub logging: LoggingCliArgs,

    /// use GUI to configure the file system
    #[arg(short, long, default_value_t = false)]
    pub no_gui: bool,
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
        self.mount.finalize();
        self.file_system.finalize();
        self.cache.finalize();
        self.logging.finalize();
    }

    fn validate(&self) -> std::result::Result<(), String> {
        self.mount
            .validate()
            .map_err(|err| format!("[Mount] {}", err))?;
        self.file_system
            .validate()
            .map_err(|err| format!("[Filesystem] {}", err))?;
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
