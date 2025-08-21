use std::path::PathBuf;

use anyhow;
use clap::Parser;
use dotenvy;

use crate::logging::{LogFormat, LogLevel, LogTargets};

pub const DEFAULT_LOG_DIR: &'static str = "./logs";
pub const DEFAULT_LOG_FILE_NAME: &'static str = "remote_fs_backend";
pub const DEFAULT_LOG_FILE_EXT: &'static str = "log";

/// Application configuration that includes logging settings.
#[derive(Parser, Debug)]
#[command(author, version, about = "Remote Filesystem Backend")]
pub struct Config {
    /// Server hostname or IP to bind to
    #[arg(short, long, env = "SERVER_HOST")]
    pub server_host: String,

    /// Server port to listen on
    #[arg(short, long, env = "SERVER_PORT")]
    pub port: u16,

    /// Log targets as comma separated list
    #[arg(short, long, value_enum, value_delimiter = ',', default_values_t = [LogTargets::All],  env = "LOG_TARGETS")]
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
}

impl Config {
    /// Parse config from CLI and environment variables
    pub fn from_args() -> anyhow::Result<Self> {
        // Load .env variables
        dotenvy::dotenv()?;
        Ok(Config::parse())
    }
}
