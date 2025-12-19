use std::path::PathBuf;

use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::{config, logging};

/// File logging configuration
#[derive(Debug)]
pub struct FileLog {
    directory: PathBuf,
    file_name: PathBuf,
    file_ext: String,
    rotation: Rotation,
}

impl FileLog {
    pub fn new() -> Self {
        FileLogBuilder::default().build()
    }

    pub fn builder() -> FileLogBuilder {
        FileLogBuilder::default()
    }

    pub fn from_config(config: &config::Config) -> Self {
        Self::builder()
            .directory(config.log_dir.clone())
            .file_name(config.log_file.clone())
            .rotation(config.log_rotation.clone())
            .build()
    }
}

impl logging::LogWriter for FileLog {
    fn create_writer(&self) -> Box<dyn std::io::Write + Send + Sync> {
        // Setup rolling file appender
        let file_appender = RollingFileAppender::builder()
            .rotation(self.rotation.clone())
            .filename_prefix(self.file_name.to_string_lossy().to_string())
            .filename_suffix(&self.file_ext)
            .build(&self.directory);

        if let Ok(writer) = file_appender {
            Box::new(writer)
        } else {
            logging::console::ConsoleLog::new()
                .with_ansi_enabled()
                .create_writer()
        }
    }

    fn ansi_enabled(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct FileLogBuilder {
    directory: Option<PathBuf>,
    file_name: Option<PathBuf>,
    file_ext: Option<String>,
    rotation: Option<Rotation>,
}

impl FileLogBuilder {
    pub fn directory(mut self, dir: Option<PathBuf>) -> Self {
        self.directory = dir;
        self
    }
    pub fn file_name(mut self, file_name: Option<PathBuf>) -> Self {
        self.file_name = file_name;
        self
    }
    pub fn file_ext(mut self, file_ext: Option<String>) -> Self {
        self.file_ext = file_ext;
        self
    }
    pub fn rotation(mut self, rotation: Option<logging::LogRotation>) -> Self {
        self.rotation = if let Some(rot) = rotation {
            Some(Rotation::from(rot))
        } else {
            None
        };
        self
    }

    pub fn build(self) -> FileLog {
        FileLog {
            directory: self
                .directory
                .unwrap_or_else(|| PathBuf::from(config::DEFAULT_LOG_DIR)),
            file_name: self
                .file_name
                .unwrap_or_else(|| PathBuf::from(config::DEFAULT_LOG_FILE_NAME)),
            file_ext: self
                .file_ext
                .unwrap_or_else(|| config::DEFAULT_LOG_FILE_EXT.to_string()),
            rotation: self.rotation.unwrap_or_else(|| {
                Rotation::from(logging::LogRotation::from(config::DEFAULT_LOG_FILE_ROT))
            }),
        }
    }
}
