use anyhow;
use clap::ValueEnum;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

use crate::config::Config;

mod console;
mod file;

/// Logging output destinations configuration
#[derive(ValueEnum, Clone, Debug, Eq, Hash, PartialEq)]
pub enum LogTargets {
    None,
    Console,
    File,
    All,
}

/// Log message format options
#[derive(ValueEnum, Clone, Debug)]
pub enum LogFormat {
    Full,
    Compact,
    Pretty,
    Json,
}

/// Log verbosity levels
#[derive(ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl ToString for LogLevel {
    fn to_string(&self) -> String {
        let current_crate = env!("CARGO_PKG_NAME");

        match self {
            LogLevel::Trace => format!("{current_crate}=trace,actix_web=info"),
            LogLevel::Debug => format!("{current_crate}=debug,actix_web=info"),
            LogLevel::Info => format!("{current_crate}=info,actix_web=info"),
            LogLevel::Warn => format!("{current_crate}=warn,actix_web=info"),
            LogLevel::Error => format!("{current_crate}=error,actix_web=info"),
        }
    }
}

/// Log rotation for file
#[derive(ValueEnum, Clone, Debug)]
pub enum LogRotation {
    Minutely,
    Hourly,
    Daily,
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

/// Main logging configuration owning all settings
#[derive(Debug)]
pub struct Logging {
    _targets: Vec<LogTargets>,
    _format: LogFormat,
    _level: LogLevel,
    _layers: LogLayer,
}

impl Logging {
    pub fn new(
        _targets: Vec<LogTargets>,
        _format: LogFormat,
        _level: LogLevel,
        _layers: LogLayer,
    ) -> Self {
        Self {
            _targets,
            _format,
            _level,
            _layers,
        }
    }

    /// Initialize logging from configuration, returning the constructed Logging struct to keep alive.
    pub fn from(config: &Config) -> anyhow::Result<Self> {
        // Build environment filter string from log level
        let env_filter = EnvFilter::try_new(config.log_level.to_string())?;

        let mut layers = LogLayer::new();
        for target in &config.log_targets {
            match target {
                LogTargets::None => {
                    layers.clear();
                    break;
                }
                LogTargets::Console => {
                    layers.add_builder(console::ConsoleLog::new().with_ansi_enabled());
                }
                LogTargets::File => {
                    layers.add_builder(file::FileLog::from_config(config));
                }
                LogTargets::All => {
                    layers.add_builder(file::FileLog::from_config(config));
                    layers.add_builder(console::ConsoleLog::new());
                }
            }
        }

        // Combine layers
        let combined_layer = layers.build_layers(&config.log_format);

        if let Some(layer) = combined_layer {
            // Build subscriber and initialize tracing system with formatting layer and env filter
            tracing_subscriber::registry()
                .with(layer)
                .with(env_filter)
                .try_init()?;
        }

        Ok(Self::new(
            config.log_targets.clone(),
            config.log_format.clone(),
            config.log_level.clone(),
            layers,
        ))
    }
}

/// Abstraction for logging output writers.
pub trait LogWriter: std::fmt::Debug + Send + Sync {
    /// Create a fresh writer instance for use by the logging system.
    ///
    /// Returns a boxed writer that is safe to use across threads.
    fn create_writer(&self) -> Box<dyn std::io::Write + Send + Sync>;

    /// Returns whether the writer supports ANSI escape codes (colors, formatting).
    ///
    /// This information can be used to conditionally enable colorized output.
    fn ansi_enabled(&self) -> bool;
}

#[derive(Debug)]
pub struct LogLayer {
    builders: Vec<Box<dyn LogWriter>>,
    guards: Vec<WorkerGuard>, // holds guards alive
}

impl LogLayer {
    pub fn new() -> Self {
        Self {
            builders: Vec::new(),
            guards: Vec::new(),
        }
    }

    pub fn add_builder<T>(&mut self, builder: T)
    where
        T: LogWriter + 'static,
    {
        self.builders.push(Box::new(builder));
    }

    /// Builds and chains all layers into a single boxed Layer, returning None if no layers were added.
    pub fn build_layers(
        &mut self,
        format: &LogFormat,
    ) -> Option<Box<dyn Layer<Registry> + Send + Sync>> {
        let mut layers = Vec::new();

        for builder in &self.builders {
            let writer = builder.create_writer();
            let (non_blocking_writer, guard) = tracing_appender::non_blocking(writer);
            let layer = build_fmt_layer(format, non_blocking_writer, builder.ansi_enabled());
            layers.push(layer);
            self.guards.push(guard); // keep guard alive to prevent drop
        }

        // Compose layers into one by chaining .and_then().boxed()`
        layers
            .into_iter()
            .reduce(|acc, layer| acc.and_then(layer).boxed())
    }

    pub fn clear(&mut self) {
        self.builders.clear();
        self.guards.clear();
    }
}

/// Builds a formatting layer according to the desired log format and writer
pub fn build_fmt_layer<T>(
    format: &LogFormat,
    writer: T,
    ansi: bool,
) -> Box<dyn Layer<Registry> + Send + Sync>
where
    T: for<'a> fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    match format {
        LogFormat::Full => fmt::layer().with_writer(writer).with_ansi(ansi).boxed(),
        LogFormat::Compact => fmt::layer()
            .compact()
            .with_writer(writer)
            .with_ansi(ansi)
            .boxed(),
        LogFormat::Pretty => fmt::layer()
            .pretty()
            .with_writer(writer)
            .with_ansi(ansi)
            .boxed(),
        LogFormat::Json => fmt::layer()
            .json()
            .with_writer(writer)
            .with_ansi(ansi)
            .boxed(),
    }
}
