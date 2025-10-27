use anyhow;
use thiserror::Error;

/// Configuration related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed loading environment variables: {0}")]
    EnvVar(String),

    #[error("Failed parsing CLI arguments")]
    ArgsParse,
}

/// Logging related errors
#[derive(Error, Debug)]
pub enum LoggingError {
    #[error("Failed to initialize logger: {0}")]
    InitFailed(String),

    #[error("Invalid logging value: {0}")]
    InvalidValue(String),
}

/// Filesystem / storage related errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Not Found: {0}")]
    NotFound(String),

    #[error("Invalid Path: {0}")]
    InvalidPath(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Operation not supported: {0}")]
    UnsupportedOperation(String),

    #[error("Conversion failed")]
    ConversionFailed,

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

/// Top-level server error enum wrapping sub-errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),

    #[error("Logging error: {0}")]
    Logging(#[from] LoggingError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
