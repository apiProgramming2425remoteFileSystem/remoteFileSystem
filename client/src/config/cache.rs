use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use super::ConfigModule;

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub use_ttl: bool,
    pub ttl: u64,
    pub policy: CachePolicy,
    pub max_size: usize,
    pub capacity: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_ttl: true,
            ttl: 300,
            policy: CachePolicy::default(),
            max_size: 1_048_576, // 1 MB
            capacity: 50,
        }
    }
}

impl ConfigModule for CacheConfig {}

/// Cache CLI arguments
#[derive(Debug, Clone, Parser, Serialize)]
pub struct CacheCliArgs {
    /// Enable local caching
    #[arg(long = "cache-enabled")]
    pub enabled: bool,

    /// Maximum number of entries in cache
    #[arg(long = "cache-capacity")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<usize>,

    /// Enable TTL eviction in cache
    #[arg(long = "cache-use-ttl")]
    pub use_ttl: bool,

    /// TTL duration in seconds (only used if --cache-use-ttl is true)
    #[arg(long = "cache-ttl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,

    /// Cache eviction policy
    #[arg(long = "cache-policy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<CachePolicy>,

    /// Maximum allowed cached file size in bytes
    #[arg(long = "cache-max-size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<usize>,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CachePolicy {
    Lru,
    Lfu,
}

impl Default for CachePolicy {
    fn default() -> Self {
        CachePolicy::Lru
    }
}
