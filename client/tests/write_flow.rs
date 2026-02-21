mod common;

use std::path::Path;
use std::process::Command;
use client::error::{FuseError, NetworkError};
use common::*;

use client::fs_model::{Attributes, FileType, SetAttr, Timestamp};
use client::network::MockRemoteStorage;

use anyhow::Result;
use mockall::predicate::*;
use tokio::fs;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use mockall::predicate;


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

#[tokio::test]
async fn test_rename_updates_cache_correctly() -> Result<()> {
    let mut mock = MockRemoteStorage::new();

    let original_name = "a.txt";
    let renamed_name = "b.txt";

    let metadata = Attributes {
        size: 123,
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

    mock.expect_get_attributes()
        .with(str::ends_with(original_name))
        .times(2)
        .returning({
            let metadata = metadata.clone();
            let mut count = 0;
            move |_| {
                count += 1;
                if count == 1 {
                    Ok(metadata.clone())
                } else {
                    Err(NetworkError::ServerError(FuseError::NotFound(
                        "File not found".to_string(),
                    )))
                }
            }
        });

    let b_counter = Arc::new(AtomicUsize::new(0));
    let b_counter_clone = b_counter.clone();

    mock.expect_get_attributes()
        .with(str::ends_with(renamed_name))
        .returning(move |_| {
            let call = b_counter_clone.fetch_add(1, Ordering::SeqCst);

            match call {
                0 => Err(NetworkError::ServerError(FuseError::NotFound(
                    "File not found".to_string(),
                ))),
                _ => panic!("b.txt getattr called more than once!"),
            }
        });

    mock.expect_rename()
        .withf(move |old_path, new_path, flags| {
            old_path.ends_with(original_name)
                && new_path.ends_with(renamed_name)
                && flags.bits() == 0
        })
        .times(1)
        .returning(|_, _, _| Ok(()));

    // --- Setup ---
    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    let client_original = app_controller.mount_point.join(original_name);
    let client_renamed = app_controller.mount_point.join(renamed_name);

    let meta_a = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_original))
        .await??;

    assert_eq!(meta_a.len(), 123);

    app_controller
        .run_with_timeout(tokio::fs::rename(&client_original, &client_renamed))
        .await??;

    let result_old = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_original))
        .await?;

    assert!(result_old.is_err());

    let meta_b = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_renamed))
        .await??;

    assert_eq!(meta_b.len(), 123);


    assert_eq!(b_counter.load(Ordering::SeqCst), 1);

    app_controller.shutdown().await?;
    Ok(())
}


#[tokio::test]
async fn test_delete_invalidates_cache() -> Result<()> {
    let mut mock = MockRemoteStorage::new();

    let file_name = "a.txt";

    let metadata = Attributes {
        size: 123,
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

    let attr_counter = Arc::new(AtomicUsize::new(0));
    let attr_counter_clone = attr_counter.clone();

    mock.expect_get_attributes()
        .with(str::ends_with(file_name))
        .returning(move |_| {
            let call = attr_counter_clone.fetch_add(1, Ordering::SeqCst);
            match call {
                0 => Ok(metadata.clone()), // primo metadata, va al server
                1 => Err(NetworkError::ServerError(FuseError::NotFound(
                    "File not found".to_string(),
                ))), // dopo delete
                _ => panic!("get_attributes called more than expected"),
            }
        });

    mock.expect_remove()
        .withf(move |path| path.ends_with(file_name))
        .times(1)
        .returning(|_| Ok(()));

    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;

    let client_file = app_controller.mount_point.join(file_name);

    let meta_1 = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_file))
        .await??;
    assert_eq!(meta_1.len(), 123);

    let meta_2 = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_file))
        .await??;
    assert_eq!(meta_2.len(), 123);

    app_controller
        .run_with_timeout(tokio::fs::remove_file(&client_file))
        .await??;

    let meta_3 = app_controller
        .run_with_timeout(tokio::fs::metadata(&client_file))
        .await?;
    assert!(meta_3.is_err());

    assert_eq!(attr_counter.load(Ordering::SeqCst), 2);

    app_controller.shutdown().await?;
    Ok(())
}


/*
#[tokio::test]
async fn test_mkdir_propagates_to_server() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let dir_name = "newdir";

    mock.expect_mkdir()
        .with(str::ends_with(dir_name))
        .times(1)
        .returning(|_| {
            Ok(Attributes {
                size: 0,
                blocks: 0,
                atime: Timestamp::new(0, 0),
                mtime: Timestamp::new(0, 0),
                ctime: Timestamp::new(0, 0),
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 4096,
            })
        });

    let app_controller = AppController::start(get_config(), mock).await?;
    let dir_path = app_controller.mount_point.join(dir_name);

    tokio::fs::create_dir(&dir_path).await?;

    app_controller.shutdown().await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_create_symlink_propagates() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let link_name = "link.txt";
    let target = "target.txt";

    mock.expect_create_symlink()
        .with(str::ends_with(link_name), eq(target))
        .times(1)
        .returning(|_, _| {
            Ok(Attributes {
                size: 0,
                blocks: 0,
                atime: Timestamp::new(0, 0),
                mtime: Timestamp::new(0, 0),
                ctime: Timestamp::new(0, 0),
                kind: FileType::Symlink,
                perm: 0o777,
                nlink: 1,
                uid: 1000,
                gid: 1000,
                rdev: 0,
                blksize: 4096,
            })
        });

    let app_controller = AppController::start(get_config(), mock).await?;
    let link_path = app_controller.mount_point.join(link_name);

    std::os::unix::fs::symlink(target, &link_path)?;

    app_controller.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_read_symlink_propagates() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let link_name = "mylink";
    let target = "real_target.txt";

    mock.expect_read_symlink()
        .with(str::ends_with(link_name))
        .times(1)
        .returning(move |_| Ok(target.to_string()));

    let app_controller = AppController::start(get_config(), mock).await?;
    let link_path = app_controller.mount_point.join(link_name);

    let resolved = tokio::fs::read_link(&link_path).await?;
    assert_eq!(resolved.to_str().unwrap(), target);

    app_controller.shutdown().await?;
    Ok(())
}
*/