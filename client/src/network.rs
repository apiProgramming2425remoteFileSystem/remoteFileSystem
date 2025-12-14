mod client;
pub mod middleware;
pub mod models;

pub use client::RemoteClient;

pub const APP_V1_BASE_URL: &str = "/api/v1";
