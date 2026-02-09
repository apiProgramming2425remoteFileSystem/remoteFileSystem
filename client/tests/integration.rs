// NOTE: The integration tests see the client as an external library.
mod common;
use common::*;

use client::network::MockRemoteStorage;

use anyhow::Result;

// TEMPLATE
#[tokio::test]
async fn test_client_with_mock() -> Result<()> {
    // Create a mock RemoteStorage client
    let mock = MockRemoteStorage::new();
    // Configure the mock to respond like the network would

    let config = get_config();

    let app_controller = AppController::start(config, mock).await?;

    // Do some operations here that would interact with the mock.
    // Use `run_with_timeout` to ensure the app doesn't crash.
    // Use tokio::fs to perform file operations on the mounted filesystem, because it async and non-blocking.
    // let health_result = app_controller.run_with_timeout(tokio::fs::<function>()).await;

    app_controller.shutdown().await?;
    Ok(())
}
