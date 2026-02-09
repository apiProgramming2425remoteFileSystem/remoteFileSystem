mod common;
use common::*;

use client::fs_model::{Attributes, FileType, Timestamp};
use client::network::MockRemoteStorage;

use anyhow::Result;
use mockall::predicate::*;
use tokio::fs;

#[tokio::test]
async fn test_read_file_with_caching_behavior() -> Result<()> {
    // Configure Mock
    let mut mock = MockRemoteStorage::new();
    let file_name = "doc.txt";
    let content = "Hello Cache!";
    let file_content = content.as_bytes();
    let content_size = file_content.len();

    // Metadata: allow multiple calls because FUSE looks up attributes frequently
    mock.expect_get_attributes()
        .with(str::ends_with(file_name))
        .returning(move |_| {
            Ok(Attributes {
                size: content_size as u64,
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

    // Data Read (Offset 0), actual data fetch. The server should be called ONLY ONCE for "doc.txt".
    // Subsequent reads should be served from the internal cache.
    mock.expect_read_file()
        .with(str::ends_with(file_name), eq(0), ge(content_size))
        .times(1)
        .returning(move |_, _, _| Ok(file_content.to_vec()));

    // EOF Check (Offset 12), the kernel reads past the content to ensure EOF.
    mock.expect_read_file()
        .with(str::ends_with(file_name), ge(content_size), always())
        .returning(move |_, _, _| Ok(vec![]));

    // Setup Environment
    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    let file_path = app_controller.mount_point.join(file_name);

    println!("Testing read caching for file: {}", file_path.display());

    // First Read: Should trigger a network call (Cache Miss)
    let content_1 = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await??;

    println!("First read content: {}", content_1);
    assert_eq!(content_1, content);

    // Second Read: Should hit the cache (No network call)
    // If the mock is called again, the test will fail due to `.times(1)` expectation.
    let content_2 = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await??;

    assert_eq!(content_2, content);

    // Cleanup
    app_controller.shutdown().await?;
    Ok(())
}
