use clap::Parser;
use serde::{Deserialize, Serialize};

use super::ConfigModule;

/// Mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub allow_root: bool,
    pub allow_other: bool,
    pub read_only: bool,
    pub privileged: bool,
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            allow_root: false,
            allow_other: false,
            read_only: false,
            privileged: false,
        }
    }
}

impl ConfigModule for MountConfig {}

/// Mount CLI arguments
#[derive(Debug, Clone, Parser, Serialize)]
pub struct MountCliArgs {
    /// Allow root user access to the mounted filesystem
    #[arg(long, num_args = 0, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_root: Option<bool>,

    /// Allow other users access to the mounted filesystem
    #[arg(long, num_args = 0, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_other: Option<bool>,

    /// Mount the filesystem as read-only
    #[arg(long, num_args = 0, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,

    /// Mount the filesystem as unprivileged
    #[arg(long, num_args = 0, default_missing_value = "true")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
}
