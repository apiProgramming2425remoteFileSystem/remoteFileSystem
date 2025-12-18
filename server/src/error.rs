use anyhow;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "message")]
/// API related errors
/// NOTE: Keep this enum in sync with the `FuseError` enum in `client/src/error.rs`
pub enum ApiError {
    // --- Authentication ---
    Unauthorized(String),
    // --- File and Path ---
    NotFound(String),
    AlreadyExists(String),
    NotADirectory(String),
    IsADirectory(String),
    // --- Permission and Security ---
    PermissionDenied(String),
    OperationNotPermitted(String),
    // --- Space and Resources ---
    StorageFull(String),
    OutOfMemory(String),
    // --- Arguments and State ---
    InvalidInput(String),
    FileTooLarge(String),
    // --- Unsupported Operations ---
    Unsupported(String),
    CrossDeviceLink(String),
    // --- I/O and Consistency ---
    IoError(String),
    TextFileBusy(String),
    // --- Lock and Concurrency ---
    ResourceBusy(String),
    TryAgain(String),
    // --- Other ---
    InternalError(String),
}

#[macro_export]
macro_rules! api_err {
    // Variant 1: Only type and static message
    ($variant:ident, $msg:expr) => {
        $crate::error::ApiError::$variant($msg.to_string())
    };
    // Variant 2: Type and formatting (like println!)
    ($variant:ident, $($arg:tt)+) => {
        $crate::error::ApiError::$variant(format!($($arg)+))
    };
}

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

    #[error("Metadata error: {0}")]
    MetadataError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Authentication related error
#[derive(Error, Debug)]
pub enum AuthenticationError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// Database related errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database creation error: {0}")]
    CreationError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error(transparent)]
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

    #[error("Authentication error: {0}")]
    AuthenticationError(#[from] AuthenticationError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<StorageError> for ApiError {
    fn from(value: StorageError) -> Self {
        match value {
            StorageError::Io(error) => api_err!(IoError, error),
            StorageError::NotFound(err) => api_err!(NotFound, err),
            StorageError::InvalidPath(err) => api_err!(InvalidInput, "Invalid path: {}", err),
            StorageError::AlreadyExists(err) => api_err!(AlreadyExists, err),
            StorageError::PermissionDenied => api_err!(PermissionDenied, "Permission denied"),
            StorageError::UnsupportedOperation(err) => {
                api_err!(Unsupported, err)
            }
            StorageError::ConversionFailed => api_err!(InternalError, "Conversion failed"),
            StorageError::MetadataError(err) => api_err!(InternalError, "Metadata error: {}", err),
            StorageError::Other(error) => {
                api_err!(InternalError, "Other storage error: {}", error)
            }
        }
    }
}

impl From<AuthenticationError> for ApiError {
    fn from(value: AuthenticationError) -> Self {
        match value {
            AuthenticationError::Unauthorized(err) => {
                api_err!(Unauthorized, err)
            }
            AuthenticationError::NotFound(err) => api_err!(NotFound, err),
        }
    }
}

impl From<DatabaseError> for ApiError {
    fn from(value: DatabaseError) -> Self {
        match value {
            DatabaseError::CreationError(err) => {
                api_err!(InternalError, "Database creation error: {}", err)
            }
            DatabaseError::ConnectionError(err) => {
                api_err!(InternalError, "Database connection error: {}", err)
            }
            DatabaseError::MigrationError(err) => {
                api_err!(InternalError, "Database migration error: {}", err)
            }
            DatabaseError::QueryError(err) => {
                api_err!(InternalError, "Database query error: {}", err)
            }
            DatabaseError::Other(err) => api_err!(InternalError, "Database other error: {}", err),
        }
    }
}
