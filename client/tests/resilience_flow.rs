mod common;
use common::*;

use client::error::FuseError;
use client::error::NetworkError;
use client::network::MockRemoteStorage;

use anyhow::Result;
use tokio::fs;

#[tokio::test]
async fn test_server_failure_returns_io_error() -> Result<()> {
    let mut mock = MockRemoteStorage::new();

    // Metadata: allow multiple calls because FUSE looks up attributes frequently
    mock.expect_get_attributes().returning(|_| {
        Err(NetworkError::ServerError(FuseError::InternalError(
            "Internal Server Error 500".to_string(),
        )))
    });

    // Scenario: The server is down or returns a 500 Error
    mock.expect_read_file().returning(|_, _, _| {
        Err(NetworkError::ServerError(FuseError::InternalError(
            "Internal Server Error 500".to_string(),
        )))
    });

    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    let file_path = app_controller.mount_point.join("broken_file.txt");

    // Execution
    let result = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await?;

    // Assertion: The client should NOT panic. It should return a clean IO error.
    assert!(result.is_err(), "Read should fail due to server error");

    // Cleanup
    app_controller.shutdown().await?;
    Ok(())
}
