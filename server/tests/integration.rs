// NOTE: The integration tests see the client as an external library.
mod common;
use common::*;

use server::config::Config;
use server::logging::{LogFormat, LogLevel, LogTargets};

use anyhow::Result;

// TEMPLATE
#[tokio::test]
async fn test_server() -> Result<()> {
    let fs_root = tempfile::tempdir()?;

    let mut config = get_config(fs_root.path());
    let (_log, http_client, app_handle) = start_server_app(config).await?;

    // Do some operations here

    Ok(())
}
