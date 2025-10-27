use anyhow;
use reqwest::Error as ReqwestError;
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

/// Daemon (service) related errors
#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("Daemon failed to start: {0}")]
    StartFailed(String),

    #[error("Signal handling error: {0}")]
    SignalError(String),
}

/// Filesystem model related errors
#[derive(Error, Debug)]
pub enum FsModelError {
    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Conversion failed")]
    ConversionFailed,

    #[error("Remote backend error: {0}")]
    Backend(#[from] anyhow::Error),
}

/// FUSE related errors
#[derive(Error, Debug)]
pub enum FuseError {
    #[error("Not Implemented")]
    NotImplemented,

    #[error("Invalid file handle: {0}")]
    InvalidFileHandle(u64),

    #[error("FUSE operation not supported")]
    UnsupportedOperation,
}

/// Network related errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Request error: {0}")]
    Request(#[from] ReqwestError),

    #[error("Timeout occurred")]
    Timeout,
}

/// Cache related errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {0}")]
    CacheMiss(String),

    #[error("Cache corruption detected")]
    Corruption,
}

/// Top-level client error enum wrapping sub-errors
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),

    #[error("Logging error: {0}")]
    Logging(#[from] LoggingError),

    #[error("Daemon error: {0}")]
    Daemon(#[from] DaemonError),

    #[error("Filesystem error: {0}")]
    FsModel(#[from] FsModelError),

    #[error("FUSE error: {0}")]
    Fuse(#[from] FuseError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
