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

    #[error("Invalid path: {0}")]
    InvalidPath(String),
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

    #[error("Daemon failed to stop: {0}")]
    StopFailed(String),

    #[error("Signal handling error: {0}")]
    SignalError(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Mount related errors
#[derive(Error, Debug)]
pub enum MountError {
    #[error("Mount failed: {0}")]
    MountFailed(String),

    #[error("Unmount failed: {0}")]
    UnmountFailed(String),

    #[error("Mountpoint not found: {0}")]
    MountpointNotFound(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Filesystem model related errors
#[derive(Error, Debug)]
pub enum FsModelError {
    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Conversion failed: {0}")]
    ConversionFailed(String),

    #[error("FileHandlers error")]
    FileHandlerError,

    #[error("Writers error")]
    WriterError,

    #[error("Remote backend error: {0}")]
    Backend(#[from] NetworkError),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
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
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Request error: {0}")]
    Request(#[from] ReqwestError),

    #[error("Timeout occurred")]
    Timeout,

    #[error("Server error occurred: {0}")]
    ServerError(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/* cache uses Option for now, can't generate errors
/// Cache related errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {0}")]
    CacheMiss(String),

    #[error("Cache corruption detected")]
    Corruption,
}
*/

/// Top-level client error enum wrapping sub-errors
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),

    #[error("Logging error: {0}")]
    Logging(#[from] LoggingError),

    #[error("Daemon error: {0}")]
    Daemon(#[from] DaemonError),

    #[error("Mount error: {0}")]
    Mount(#[from] MountError),

    #[error("Filesystem error: {0}")]
    FsModel(#[from] FsModelError),

    #[error("FUSE error: {0}")]
    Fuse(#[from] FuseError),

    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    //#[error("Cache error: {0}")]
    //Cache(#[from] CacheError),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
