use std::time::Instant;

use clap::ValueEnum;
use tracing::{Id, Subscriber, span};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt, layer};

use crate::config::logging::{LogFormat, LogLevel, LogTargets, LoggingConfig};
use crate::error::LoggingError;

mod console;
mod file;

type Result<T> = std::result::Result<T, LoggingError>;

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
    pub fn from(config: &LoggingConfig) -> Result<Self> {
        // Build environment filter string from log level
        let env_filter = EnvFilter::try_new(config.log_level.to_string())
            .map_err(|err| LoggingError::InvalidValue(err.to_string()))?;

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
                .with(TimerLayer)
                .with(env_filter)
                .try_init()
                .map_err(|err| LoggingError::InitFailed(err.to_string()))?;
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

struct StartTime(Instant);
struct TimerLayer;

impl<S> Layer<S> for TimerLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    // When a span is created, we save the current time in its extensions
    fn on_new_span(&self, _attrs: &span::Attributes<'_>, id: &Id, ctx: layer::Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            // BEST PRACTICE: Use a wrapper struct instead of a raw Instant
            // to avoid conflicts with other libraries that might also store an Instant.
            span.extensions_mut().insert(StartTime(Instant::now()));
        }
    }

    // When the span closes, we retrieve the time, calculate the duration, and log it in DEBUG
    fn on_close(&self, id: tracing::Id, ctx: layer::Context<'_, S>) {
        let Some(log_span) = ctx.span(&id) else {
            return;
        };
        let metadata = log_span.metadata();
        let extensions = log_span.extensions();

        let Some(start_time) = extensions.get::<StartTime>() else {
            return;
        };

        let elapsed = start_time.0.elapsed();

        tracing::debug!(
            "{}::{} total_time: {:?}",
            metadata.target(),
            metadata.name(),
            elapsed,
        );
    }
}
