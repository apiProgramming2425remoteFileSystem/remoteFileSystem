use anyhow;
use reqwest_middleware::Error as ReqwestError;
use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    #[error("Missing argument: {0}")]
    MissingArgument(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Configuration related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed loading environment variables: {0}")]
    EnvVar(String),

    #[error("Failed parsing CLI arguments")]
    ArgsParse,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
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

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Mount related errors
#[derive(Error, Debug)]
pub enum MountError {
    #[error("Mount failed: {0}")]
    MountFailed(String),

    #[error("Unmount failed: {0}")]
    UnmountFailed(String),

    #[error("Mount point not found: {0}")]
    MountPointNotFound(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error(transparent)]
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

    #[error("No data: {0}")]
    NoData(String),

    #[error("Remote Server error: {0}")]
    ServerError(#[from] NetworkError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Error, Debug, Deserialize)]
#[serde(tag = "type", content = "message")]
/// FUSE related errors
/// NOTE: Keep this enum in sync with the `ApiError` enum in `server/src/error.rs`
pub enum FuseError {
    // --- Authentication ---
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    // --- File and Path ---
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Already Exists: {0}")]
    AlreadyExists(String),
    #[error("Not a Directory: {0}")]
    NotADirectory(String),
    #[error("Is a Directory: {0}")]
    IsADirectory(String),

    // --- Permission and Security ---
    #[error("Permission Denied: {0}")]
    PermissionDenied(String),
    #[error("Operation Not Permitted: {0}")]
    OperationNotPermitted(String),

    // --- Space and Resources ---
    #[error("Storage Full: {0}")]
    StorageFull(String),
    #[error("Out of Memory: {0}")]
    OutOfMemory(String),

    // --- Arguments and State ---
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
    #[error("File Too Large: {0}")]
    FileTooLarge(String),

    // --- Unsupported Operations ---
    #[error("Unsupported: {0}")]
    Unsupported(String),
    #[error("Cross Device Link: {0}")]
    CrossDeviceLink(String),

    // --- I/O and Consistency ---
    #[error("I/O Error: {0}")]
    IoError(String),
    #[error("Text File Busy: {0}")]
    TextFileBusy(String),

    // --- Lock and Concurrency ---
    #[error("Resource Busy: {0}")]
    ResourceBusy(String),
    #[error("Try Again: {0}")]
    TryAgain(String),

    // --- Other ---
    #[error("Internal Error: {0}")]
    InternalError(String),

    // --- Additional FUSE specific errors ---
    #[error("Not Implemented")]
    NotImplemented,
    #[error("Invalid file handle: {0}")]
    InvalidFileHandle(u64),
}

/// Network related errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Request error: {0}")]
    Request(#[from] ReqwestError),

    #[error("API error: {0:?}")]
    ServerError(FuseError),

    #[error("Unexpected Server Response: {0}")]
    UnexpectedResponse(String), // When the server does not send valid JSON

    #[error(transparent)]
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
pub enum RfsClientError {
    #[error("Command error: {0}")]
    Command(#[from] CommandError),

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
