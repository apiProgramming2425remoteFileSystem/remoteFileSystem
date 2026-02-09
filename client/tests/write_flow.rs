mod common;
use client::error::{FuseError, NetworkError};
use common::*;

use client::fs_model::{Attributes, FileType, SetAttr, Timestamp};
use client::network::MockRemoteStorage;

use anyhow::Result;
use mockall::predicate::*;
use tokio::fs;

#[tokio::test]
async fn test_write_file_uploads_to_server() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let file_name = "new_file.txt";
    let content_str = "New Data From FUSE";
    let expected_content = content_str.as_bytes().to_vec();
    let content_size = expected_content.len();

    // Metadata Lookup
    // The kernel checks if the file exists before creating it.
    // Return "NotFound" so the kernel proceeds to CREATE it.
    mock.expect_get_attributes()
        .with(mockall::predicate::str::ends_with(file_name))
        .returning(move |_| {
            Err(NetworkError::ServerError(FuseError::NotFound(
                "File not found".to_string(),
            )))
        });

    // Create File (Write with 0 bytes)
    // Only strictly necessary if your `create` handler calls `write_file` with empty data.
    // If your client separates `create_file` and `write_file`, this might need to be `expect_create_file`.
    mock.expect_write_file()
        .with(
            mockall::predicate::str::ends_with(file_name),
            mockall::predicate::eq(0),      // Offset 0
            mockall::predicate::eq(vec![]), // Empty data (Creation)
        )
        .times(1)
        .returning(move |_, _, _| {
            Ok(Attributes {
                size: 0, // Initially empty
                blocks: 0,
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
            })
        });

    // Write Actual Data
    let content_for_write = expected_content.clone();

    mock.expect_write_file()
        .with(
            mockall::predicate::str::ends_with(file_name),
            mockall::predicate::eq(0),                // Offset 0
            mockall::predicate::eq(expected_content), // Expect specific content
        )
        .times(1)
        .returning(move |_, _, _| {
            Ok(Attributes {
                size: content_size as u64, // New size
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
            })
        });

    // Setup
    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    let file_path = app_controller.mount_point.join(file_name);

    // Execute Write
    app_controller
        .run_with_timeout(fs::write(&file_path, content_str))
        .await??;

    // Cleanup
    app_controller.shutdown().await?;
    Ok(())
}
