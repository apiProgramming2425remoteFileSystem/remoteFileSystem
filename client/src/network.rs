mod client;
pub mod middleware;
pub mod models;

pub use client::RemoteClient;

use async_trait;
use models::*;
use std::fmt::Debug;

use crate::error::NetworkError;
use crate::fs_model::{Attributes, RenameFlags, SetAttr, Stats};

pub const APP_V1_BASE_URL: &str = "/api/v1";

type Result<T> = std::result::Result<T, NetworkError>;

#[cfg(any(test, feature = "int-tests"))]
use mockall::automock;

#[cfg_attr(any(test, feature = "int-tests"), automock)]
#[async_trait::async_trait]
pub trait RemoteStorage: Debug + Send + Sync + 'static {
    async fn health_check(&self) -> Result<()>;

    // AUTHENTICATION MANAGEMENT
    async fn login(&self, username: String, password: String) -> Result<String>;
    async fn logout(&self) -> Result<()>;

    // ATTRIBUTES
    async fn get_attributes(&self, path: &str) -> Result<Attributes>;
    async fn set_attributes(&self, path: &str, new_attributes: SetAttr) -> Result<Attributes>;

    // XATTRIBUTES
    async fn get_x_attributes(&self, path: &str, name: &str) -> Result<Option<Xattributes>>;
    async fn set_x_attributes(&self, path: &str, name: &str, xattributes: &[u8]) -> Result<()>;
    async fn list_x_attributes(&self, path: &str) -> Result<Vec<String>>;
    async fn remove_x_attributes(&self, path: &str, name: &str) -> Result<()>;

    // PERMISSIONS AND STATS
    async fn get_permissions(&self, path: &str, mask: u32) -> Result<()>;
    async fn get_stats(&self, path: &str) -> Result<Stats>;

    // FILESYSTEM OPERATIONS
    async fn list_path(&self, path: &str) -> Result<Vec<SerializableFSItem>>;
    async fn read_file(&self, path: &str, offset: usize, size: usize) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &str, offset: usize, data: &[u8]) -> Result<Attributes>;
    async fn mkdir(&self, path: &str) -> Result<Attributes>;
    async fn rename(&self, old_path: &str, new_path: &str, flags: RenameFlags) -> Result<()>;
    async fn remove(&self, path: &str) -> Result<()>;
    //async fn resolve_child(&self, path: &str) -> Result<Attributes>;
    async fn create_symlink(&self, path: &str, target: &str) -> Result<Attributes>;
    async fn read_symlink(&self, path: &str) -> Result<String>;
}
