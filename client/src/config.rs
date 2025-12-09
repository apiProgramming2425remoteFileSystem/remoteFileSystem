use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use crate::error::ConfigError;
use crate::logging::{LogFormat, LogLevel, LogRotation, LogTargets};
use crate::cache::{CacheConfig, CachePolicy};

use clap::Parser;
use dotenvy;

pub const DEFAULT_LOG_DIR: &'static str = "./logs";
pub const DEFAULT_LOG_FILE_NAME: &'static str = "remote_fs_client";
pub const DEFAULT_LOG_FILE_EXT: &'static str = "log";
pub const DEFAULT_LOG_FILE_ROT: &'static str = "never";

type Result<T> = std::result::Result<T, ConfigError>;

/// Application configuration that includes logging settings.
#[derive(Parser, Debug)]
#[command(author, version, about = "Remote Filesystem Client")]
pub struct Config {
    /// Mountpoint path (e.g /mnt/remote-fs)
    #[arg(short, long, env = "MOUNT_POINT")]
    pub mountpoint: PathBuf,

    /// Remote server base URL (e.g. http://localhost:8080/)
    #[arg(short, long, env = "SERVER_URL")]
    pub server_url: String,

    /// Enable local caching
    #[arg(short, long, default_value_t = true)]
    pub cache_enabled: bool,

    /// Maximum number of entries in cache
    #[arg(long, default_value_t = 50)]
    pub cache_capacity: usize,

    /// Enable TTL eviction in cache
    #[arg(long, default_value_t = true)]
    pub cache_use_ttl: bool,

    /// TTL duration in seconds (only used if --cache-use-ttl is true)
    #[arg(long, default_value_t = 300)]
    pub cache_ttl_seconds: u64,

    /// Cache eviction policy
    #[arg(long, value_enum, default_value_t = CachePolicy::Lru)]
    pub cache_policy: CachePolicy,

    /// Maximum allowed cached file size in bytes
    #[arg(long, default_value_t = 1_048_576)]
    pub cache_max_size: usize,

    /// Log targets as comma separated list
    #[arg(short, long, value_enum, value_delimiter = ',', default_values_t = [LogTargets::All], env = "LOG_TARGETS")]
    pub log_targets: Vec<LogTargets>,

    /// Log format
    #[arg(long, value_enum, default_value_t = LogFormat::Full, env = "LOG_FORMAT")]
    pub log_format: LogFormat,

    /// Log level filter
    #[arg(long, value_enum, default_value_t = LogLevel::Info, env = "LOG_LEVEL")]
    pub log_level: LogLevel,

    /// Optional path for log directory. Defaults to [`DEFAULT_LOG_DIR`] if needed.
    #[arg(
        long,
        default_value_ifs([
            ("log_targets", "all", Some(DEFAULT_LOG_DIR)),
            ("log_targets", "file", Some(DEFAULT_LOG_DIR))
        ]),
        env = "LOG_DIR"
    )]
    pub log_dir: Option<PathBuf>,

    /// Optional log file name. Defaults to [`DEFAULT_LOG_FILE_NAME`] if needed.
    #[arg(
        long,
        default_value_ifs([
            ("log_targets", "all", Some(DEFAULT_LOG_FILE_NAME)),
            ("log_targets", "file", Some(DEFAULT_LOG_FILE_NAME))
        ]), env = "LOG_FILE")]
    pub log_file: Option<PathBuf>,

    /// Optional log rotation policy. Defaults to [`DEFAULT_LOG_FILE_ROT`] if needed
    #[arg(
        long,
        default_value_ifs([
            ("log_targets", "all", Some(DEFAULT_LOG_FILE_ROT)),
            ("log_targets", "file", Some(DEFAULT_LOG_FILE_ROT))
        ]), env = "LOG_ROTATION")]
    pub log_rotation: Option<LogRotation>,
}

impl Config {
    /// Parse config from CLI and environment variables
    pub fn from_args() -> Result<Self> {
        // Load .env variables
        // dotenvy::dotenv().map_err(|err| ConfigError::EnvVar(err.to_string()))?;
        let _ = dotenvy::dotenv().map_err(|err| ConfigError::EnvVar(err.to_string()));

        let mut config = Config::parse();
        config.normalize_targets();

        Ok(config)
    }

    /// Normalize log_targets in place by deduplicating and handling special cases
    fn normalize_targets(&mut self) {
        let mut set: HashSet<LogTargets> = self.log_targets.drain(..).collect();

        if set.contains(&LogTargets::None) {
            set.clear();
        }

        if set.contains(&LogTargets::All) {
            set.remove(&LogTargets::Console);
            set.remove(&LogTargets::File);
        }

        self.log_targets = set.into_iter().collect();
    }

    pub fn cache_config(&self) -> CacheConfig {
        CacheConfig {
            enabled: self.cache_enabled,
            use_ttl: self.cache_use_ttl,
            ttl: Duration::from_secs(self.cache_ttl_seconds),
            policy: self.cache_policy,
            max_size: self.cache_max_size,
            capacity: self.cache_capacity,
        }
    }
}

