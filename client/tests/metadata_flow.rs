mod common;
use common::*;

use client::fs_model::{Attributes, FileType, SetAttr, Timestamp};
use client::network::MockRemoteStorage;

use anyhow::Result;
use mockall::predicate::*;
use std::os::unix::fs::PermissionsExt;
use tokio::fs;
use tokio_test::assert_ok;

#[tokio::test]
async fn test_stat_returns_correct_attributes() -> Result<()> {
    // Configure Mock
    let mut mock = MockRemoteStorage::new();

    // Define the metadata we expect the server to return.
    // E.g., a file that is 1024 bytes large and has 644 permissions (rw-r--r--).
    let server_metadata = Attributes {
        size: 1024,
        blocks: 1,
        atime: Timestamp::new(0, 0),
        mtime: Timestamp::new(0, 0),
        ctime: Timestamp::new(0, 0),
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        blksize: 4096,
    };

    let file_name = "fake_image.png";

    // Expectation: When user runs `stat` or `ls`, the client asks the server for metadata.
    mock.expect_get_attributes()
        .with(str::ends_with(file_name))
        .times(1)
        .returning(move |_| Ok(server_metadata));

    // Setup Environment
    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    // Execution (Metadata Check)
    let file_path = app_controller.mount_point.join(file_name);

    // We use tokio::fs::metadata to mimic the `stat` syscall.
    let metadata = assert_ok!(
        app_controller
            .run_with_timeout(fs::metadata(&file_path))
            .await?
    );

    // Verify file size matches what the mock returned
    assert_eq!(
        metadata.len(),
        1024,
        "File size should match server metadata"
    );

    // Verify permissions (using bitwise masking to ignore file type bits)
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o644, "Permissions should be rw-r--r--");

    // Trigger the shutdown
    app_controller.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_chmod_propagates_to_server() -> Result<()> {
    let config = get_config();
    let mut mock = MockRemoteStorage::new();

    let server_metadata = Attributes {
        size: 9999,
        blocks: 1,
        atime: Timestamp::new(0, 0),
        mtime: Timestamp::new(0, 0),
        ctime: Timestamp::new(0, 0),
        kind: FileType::RegularFile,
        perm: 0o755,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        blksize: 4096,
    };
    let mut updated = server_metadata.clone();
    updated.perm = 0o755;

    // Expectation: User runs `chmod 755 script.sh`.
    // The client should call set_metadata on the server with the new mode.
    mock.expect_set_attributes()
        .with(
            str::ends_with("script.sh"),
            function(|meta: &SetAttr| {
                // Check if the requested mode is indeed 755
                if let Some(mode) = meta.mode {
                    (mode & 0o777) == 0o755
                } else {
                    false
                }
            }),
        )
        // .times(1)
        .returning(move |_, _| Ok(server_metadata));

    // Also, when the client fetches attributes after chmod, return updated perms
    mock.expect_get_attributes()
        .with(str::ends_with("script.sh"))
        .returning(move |_| Ok(updated));

    let app_controller = AppController::start(config, mock).await?;

    // Execution (Chmod)
    let file_path = app_controller.mount_point.join("script.sh");

    // Set permissions to rwxr-xr-x (755)
    let new_perms = std::fs::Permissions::from_mode(0o755);
    assert_ok!(
        app_controller
            .run_with_timeout(fs::set_permissions(&file_path, new_perms))
            .await?
    );

    // Cleanup
    app_controller.shutdown().await?;

    Ok(())
}
