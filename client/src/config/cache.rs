use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::config::DEFAULT_CACHE_CAPACITY;

use super::{ConfigModule, DEFAULT_CACHE_MAX_SIZE, DEFAULT_CACHE_TTL};

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
            ttl: DEFAULT_CACHE_TTL,
            policy: CachePolicy::default(),
            max_size: DEFAULT_CACHE_MAX_SIZE,
            capacity: DEFAULT_CACHE_CAPACITY,
        }
    }
}

impl ConfigModule for CacheConfig {}

/// Cache CLI arguments
#[derive(Debug, Clone, Parser, Serialize)]
pub struct CacheCliArgs {
    /// Disable local caching
    #[arg(long = "no-cache", num_args = 0, default_missing_value = "false")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Maximum number of entries in cache
    #[arg(long = "cache-capacity")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<usize>,

    /// Disable TTL eviction in cache
    #[arg(long = "cache-no-ttl", num_args = 0, default_missing_value = "false")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_ttl: Option<bool>,

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

// REVIEW: add ttl here ans set it as default. Remove no_ttl from CLI args?
#[derive(Debug, Default, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CachePolicy {
    #[default]
    Lru,
    Lfu,
}

impl ToString for CachePolicy{
    fn to_string(&self) -> String {
        match self {
            CachePolicy::Lru => String::from("LRU"),
            CachePolicy::Lfu => String::from("LFU"),
        }
    }
}
