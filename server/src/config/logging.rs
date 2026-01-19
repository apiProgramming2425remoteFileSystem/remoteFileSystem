use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use tracing_appender::rolling::Rotation;

use super::{ConfigModule, DEFAULT_LOG_DIR, DEFAULT_LOG_FILE_NAME};
use crate::util;

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(deserialize_with = "util::deserialize_flexible_vec")]
    pub log_targets: Vec<LogTargets>,
    pub log_format: LogFormat,
    pub log_level: LogLevel,
    pub log_dir: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub log_rotation: Option<LogRotation>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_targets: vec![LogTargets::default()],
            log_format: LogFormat::default(),
            log_level: LogLevel::default(),
            log_dir: Some(PathBuf::from(DEFAULT_LOG_DIR)), // None,
            log_file: Some(PathBuf::from(DEFAULT_LOG_FILE_NAME)), // None,
            log_rotation: Some(LogRotation::default()),    //None,
        }
    }
}

impl ConfigModule for LoggingConfig {
    fn finalize(&mut self) {
        // Canonicalize paths if present
        self.log_dir = util::normalize_optional_path(&self.log_dir);
        self.log_file = util::normalize_optional_path(&self.log_file);

        // Deduplicate log_targets and handle special cases
        self.log_targets.sort();
        self.log_targets.dedup();

        if self.log_targets.contains(&LogTargets::None) {
            self.log_targets.clear();
        }
        if self.log_targets.contains(&LogTargets::All) {
            self.log_targets = vec![LogTargets::All];
        }

        // Determine if we need to set defaults for file logging
        let needs_file = self.log_targets.contains(&LogTargets::All)
            || self.log_targets.contains(&LogTargets::File);

        if needs_file {
            // Apply defaults ONLY if the user (or config file) has not specified anything
            if self.log_dir.is_none() {
                self.log_dir = Some(PathBuf::from(DEFAULT_LOG_DIR));
            }
            if self.log_file.is_none() {
                self.log_file = Some(PathBuf::from(DEFAULT_LOG_FILE_NAME));
            }
            if self.log_rotation.is_none() {
                self.log_rotation = Some(LogRotation::default());
            }
        } else {
            // Clear file-related settings if file logging is not needed
            self.log_dir = None;
            self.log_file = None;
            self.log_rotation = None;
        }
    }
}

/// Logging CLI arguments
#[derive(Debug, Clone, Parser, Serialize)]
pub struct LoggingCliArgs {
    /// Log targets as comma separated list
    #[arg(long, value_enum, value_delimiter = ',')]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_targets: Option<Vec<LogTargets>>,

    /// Log format
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_format: Option<LogFormat>,

    /// Log level filter
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<LogLevel>,

    /// Optional path for log directory. Defaults to [`DEFAULT_LOG_DIR`] if needed.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_dir: Option<PathBuf>,

    /// Optional log file name. Defaults to [`DEFAULT_LOG_FILE_NAME`] if needed.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<PathBuf>,

    /// Optional log rotation policy. Defaults to [`DEFAULT_LOG_FILE_ROT`] if needed
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_rotation: Option<LogRotation>,
}

/// Logging output destinations configuration
#[derive(
    ValueEnum, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "lowercase")]
pub enum LogTargets {
    None,
    Console,
    File,
    #[default]
    All,
}

impl FromStr for LogTargets {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "console" => Ok(LogTargets::Console),
            "file" => Ok(LogTargets::File),
            "none" => Ok(LogTargets::None),
            "all" => Ok(LogTargets::All),
            _ => Err(format!("Target invalid: {}", s)),
        }
    }
}

impl std::fmt::Display for LogTargets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogTargets::None => "none",
            LogTargets::Console => "console",
            LogTargets::File => "file",
            LogTargets::All => "all",
        };
        write!(f, "{}", s)
    }
}

/// Log message format options
#[derive(ValueEnum, Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Full,
    Compact,
    Pretty,
    Json,
}

/// Log verbosity levels
#[derive(ValueEnum, Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    #[default]
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let current_crate = env!("CARGO_PKG_NAME");

        let s = match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };

        write!(f, "{current_crate}={s},actix_web=info")
    }
}

/// Log rotation for file
#[derive(ValueEnum, Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
    Minutely,
    Hourly,
    Daily,
    #[default]
    Never,
}

impl From<&str> for LogRotation {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "minutely" => LogRotation::Minutely,
            "hourly" => LogRotation::Hourly,
            "daily" => LogRotation::Daily,
            "never" => LogRotation::Never,
            _ => LogRotation::Never,
        }
    }
}

impl From<LogRotation> for Rotation {
    fn from(value: LogRotation) -> Self {
        match value {
            LogRotation::Minutely => Rotation::MINUTELY,
            LogRotation::Hourly => Rotation::HOURLY,
            LogRotation::Daily => Rotation::DAILY,
            LogRotation::Never => Rotation::NEVER,
        }
    }
}
