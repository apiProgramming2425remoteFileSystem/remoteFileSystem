mod common;

use common::*;
use std::collections::HashMap;

use client::fs_model::{Attributes, FileType, Timestamp};
use client::network::MockRemoteStorage;

use anyhow::Result;
use client::config::CachePolicy;
use mockall::predicate::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::fs;
use tokio::time::{Duration, sleep};

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

#[tokio::test]
async fn test_read_file_cache_ttl_expiration() -> Result<()> {
    let mut mock = MockRemoteStorage::new();

    let file_name = "ttl.txt";
    let content_a = b"A";
    let content_b = b"B";

    let call_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = call_counter.clone();

    mock.expect_get_attributes()
        .with(str::ends_with(file_name))
        .returning(move |_| {
            Ok(Attributes {
                size: 1,
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

    mock.expect_read_file()
        .with(str::ends_with(file_name), eq(0), ge(1))
        .times(2)
        .returning(move |_, _, _| {
            let call_index = counter_clone.fetch_add(1, Ordering::SeqCst);

            match call_index {
                0 => Ok(content_a.to_vec()), // prima chiamata
                1 => Ok(content_b.to_vec()), // seconda chiamata (post TTL)
                _ => panic!("read_file called more than twice"),
            }
        });

    mock.expect_read_file()
        .with(str::ends_with(file_name), ge(1), always())
        .returning(|_, _, _| Ok(vec![]));

    let mut config = get_config();
    config.cache.ttl = 1;

    let app_controller = AppController::start(config, mock).await?;
    let file_path = app_controller.mount_point.join(file_name);

    let read_1 = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await??;
    assert_eq!(read_1, "A");

    let read_2 = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await??;
    assert_eq!(read_2, "A");

    assert_eq!(call_counter.load(Ordering::SeqCst), 1);

    sleep(Duration::from_secs(2)).await;

    let read_3 = app_controller
        .run_with_timeout(fs::read_to_string(&file_path))
        .await??;
    assert_eq!(read_3, "B");

    assert_eq!(call_counter.load(Ordering::SeqCst), 2);

    app_controller.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_concurrent_reads_use_shared_cache_and_buffer() -> Result<()> {
    use std::sync::Arc;
    use tokio::task;

    // --- Setup Mock ---
    let mut mock = MockRemoteStorage::new();
    let file_name = "concurrent.txt";
    let content = "Concurrent Cache Test!";
    let file_content = content.as_bytes();
    let content_size = file_content.len();

    // Metadata: FUSE chiede attributi, ok chiamare più volte
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

    // Data read: la chiamata al server deve avvenire una sola volta
    mock.expect_read_file()
        .with(str::ends_with(file_name), eq(0), ge(content_size))
        .times(1)
        .returning(move |_, _, _| Ok(file_content.to_vec()));

    // EOF
    mock.expect_read_file()
        .with(str::ends_with(file_name), ge(content_size), always())
        .returning(move |_, _, _| Ok(vec![]));

    // --- Setup AppController ---
    let config = get_config();
    let app_controller = Arc::new(AppController::start(config, mock).await?);
    let file_path = app_controller.mount_point.join(file_name);

    println!("Testing concurrent reads on file: {}", file_path.display());

    // --- Spawn concurrent reads ---
    let mut handles = vec![];
    for _ in 0..8 {
        let ac = app_controller.clone();
        let fp = file_path.clone();
        handles.push(task::spawn(async move {
            let read_content = ac.run_with_timeout(fs::read_to_string(&fp)).await??;
            Ok::<_, anyhow::Error>(read_content)
        }));
    }

    // --- Collect results ---
    for handle in handles {
        let read_result = handle.await??;
        assert_eq!(read_result, content);
    }

    let ac = Arc::try_unwrap(app_controller).expect("unwrapping Arc: must have no other refs");
    ac.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_read_large_file_buffering() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let file_name = "bigfile.dat";
    let file_size = 3 * 1024 * 1024; // 3MB
    let buffer_size = 2 * 1024 * 1024; // 2MB

    // Fill file with dummy content
    let content: Vec<u8> = (0..file_size).map(|i| (i % 256) as u8).collect();

    // Metadata lookup
    mock.expect_get_attributes()
        .with(str::ends_with(file_name))
        .returning(move |_| {
            Ok(Attributes {
                size: file_size as u64,
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

    let call_count = Arc::new(AtomicUsize::new(0));
    let content_clone = content.clone();

    mock.expect_read_file()
        .with(str::ends_with(file_name), always(), always())
        .times(2)
        .returning({
            let call_count = call_count.clone();
            move |_, offset, size| {
                let call_index = call_count.fetch_add(1, Ordering::SeqCst);
                match call_index {
                    0 => Ok(content_clone[0..buffer_size].to_vec()),
                    1 => Ok(content_clone[buffer_size..].to_vec()),
                    _ => panic!("read_file called more than twice"),
                }
            }
        });

    let config = get_config();
    let app_controller = AppController::start(config, mock).await?;
    let file_path = app_controller.mount_point.join(file_name);

    let read_content = app_controller
        .run_with_timeout(tokio::fs::read(&file_path))
        .await??;

    assert_eq!(read_content.len(), file_size);
    assert_eq!(read_content, content);

    app_controller.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_cache_eviction_lru() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let file_names = ["file1.txt", "file2.txt", "file3.txt"];
    let content: Vec<u8> = b"data".to_vec();

    let counters: Arc<HashMap<&'static str, AtomicUsize>> = Arc::new(
        file_names
            .iter()
            .map(|&name| (name, AtomicUsize::new(0)))
            .collect(),
    );

    for name in &file_names {
        mock.expect_get_attributes()
            .with(str::ends_with(*name))
            .returning({
                let c_len = content.len();
                move |_| {
                    Ok(Attributes {
                        size: c_len as u64,
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
                }
            });

        let c = content.clone();
        let counters_clone = counters.clone();
        let file = *name;

        mock.expect_read_file()
            .with(str::ends_with(*name), always(), always())
            .returning(move |_, _, _| {
                counters_clone[file].fetch_add(1, Ordering::SeqCst);
                Ok(c.clone())
            });
    }

    let mut config = get_config();
    config.cache.capacity = 2;
    config.cache.policy = CachePolicy::Lru;

    let app_controller = AppController::start(config, mock).await?;

    // file1, file2
    for name in &file_names[0..2] {
        let path = app_controller.mount_point.join(name);
        app_controller
            .run_with_timeout(tokio::fs::read(&path))
            .await??;
    }

    // file3 -> evict file1
    let path3 = app_controller.mount_point.join(file_names[2]);
    app_controller
        .run_with_timeout(tokio::fs::read(&path3))
        .await??;

    // re-read file1 -> must hit server again
    let path1 = app_controller.mount_point.join(file_names[0]);
    app_controller
        .run_with_timeout(tokio::fs::read(&path1))
        .await??;

    assert_eq!(counters["file1.txt"].load(Ordering::SeqCst), 2);
    assert_eq!(counters["file2.txt"].load(Ordering::SeqCst), 1);
    assert_eq!(counters["file3.txt"].load(Ordering::SeqCst), 1);

    app_controller.shutdown().await?;
    Ok(())
}
#[tokio::test]
async fn test_cache_eviction_lfu() -> Result<()> {
    let mut mock = MockRemoteStorage::new();
    let file_names = ["fileA.txt", "fileB.txt", "fileC.txt"];
    let content: Vec<u8> = b"data".to_vec();

    let counters: Arc<HashMap<&'static str, AtomicUsize>> = Arc::new(
        file_names
            .iter()
            .map(|&name| (name, AtomicUsize::new(0)))
            .collect(),
    );

    for name in &file_names {
        mock.expect_get_attributes()
            .with(str::ends_with(*name))
            .returning({
                let c_len = content.len();
                move |_| {
                    Ok(Attributes {
                        size: c_len as u64,
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
                }
            });

        let c = content.clone();
        let counters_clone = counters.clone();
        let file = *name;

        mock.expect_read_file()
            .with(str::ends_with(*name), always(), always())
            .returning(move |_, _, _| {
                counters_clone[file].fetch_add(1, Ordering::SeqCst);
                Ok(c.clone())
            });
    }

    let mut config = get_config();
    config.cache.capacity = 2;
    config.cache.policy = CachePolicy::Lfu;

    let app_controller = AppController::start(config, mock).await?;

    // fileA 3 times -> high frequency
    let path_a = app_controller.mount_point.join(file_names[0]);
    for _ in 0..3 {
        app_controller
            .run_with_timeout(tokio::fs::read(&path_a))
            .await??;
    }

    // fileB 1 time
    let path_b = app_controller.mount_point.join(file_names[1]);
    app_controller
        .run_with_timeout(tokio::fs::read(&path_b))
        .await??;

    // fileC -> evict fileB
    let path_c = app_controller.mount_point.join(file_names[2]);
    app_controller
        .run_with_timeout(tokio::fs::read(&path_c))
        .await??;

    // re-read fileB -> must hit server again
    app_controller
        .run_with_timeout(tokio::fs::read(&path_b))
        .await??;

    assert_eq!(counters["fileA.txt"].load(Ordering::SeqCst), 1);
    assert_eq!(counters["fileB.txt"].load(Ordering::SeqCst), 2);
    assert_eq!(counters["fileC.txt"].load(Ordering::SeqCst), 1);

    app_controller.shutdown().await?;
    Ok(())
}
