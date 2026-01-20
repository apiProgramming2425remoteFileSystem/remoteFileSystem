use clap::Parser;
use serde::{Deserialize, Serialize};

use super::{ConfigModule, DEFAULT_BUFFER_SIZE, DEFAULT_MAX_PAGES, DEFAULT_PAGE_SIZE};

/// Filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    pub xattr_enable: bool,
    pub page_size: usize,
    pub max_pages: usize,
    pub buffer_size: usize,
}

impl Default for FsConfig {
    fn default() -> Self {
        Self {
            xattr_enable: false,
            page_size: DEFAULT_PAGE_SIZE,
            max_pages: DEFAULT_MAX_PAGES,
            buffer_size: DEFAULT_BUFFER_SIZE,
        }
    }
}

impl ConfigModule for FsConfig {}

/// Filesystem CLI arguments
#[derive(Debug, Clone, Parser, Serialize)]
pub struct FsCliArgs {
    /// Disable xattributes
    #[arg(long = "no-xattr", num_args = 0, default_missing_value = "false")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xattr_enable: Option<bool>,

    /// Page size in bytes
    #[arg(long = "fs-page-size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<usize>,

    /// Maximum number of pages in memory
    #[arg(long = "fs-max-pages")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<usize>,

    /// Buffer size for file operations in bytes
    #[arg(long = "fs-buffer-size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<usize>,
}
