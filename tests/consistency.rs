use anyhow::{Result, anyhow};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::Duration;

mod common;
use common::*;

#[test]
fn test_cache_invalidation() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // 1 second TTL for testing
    sys_build.client.arg_pair("--cache-ttl", "1");

    let ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("No mount"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("No server"))?;

    let file_name = "config.json";
    let contents = r#"{"version": 1}"#;
    let contents_updated = r#"{"version": 2}"#;

    let client_path = mount_point.join(file_name);

    // Create file via Client (Populates Cache)
    fs::write(&client_path, contents)?;

    // Read back to confirm cache is hot
    let content = fs::read_to_string(&client_path)?;
    assert_eq!(content, contents);
    // BACKDOOR UPDATE: Modify the file directly on the server storage
    // This simulates another user changing the file remotely.
    let server_file = server_root.join(file_name);
    fs::write(&server_file, contents_updated)?;

    // Immediate read might still return version 1 (Allowed behavior if caching is on)
    let immediate = fs::read_to_string(&client_path)?;
    assert_eq!(
        immediate, contents,
        "Client should have served cached data immediately"
    );

    // Wait for TTL to expire
    thread::sleep(Duration::from_secs(2));

    // Verify Fresh Read
    // The client should now invalidate its cache and fetch from server
    let fresh = fs::read_to_string(&client_path)?;
    assert_eq!(
        fresh, contents_updated,
        "Client returned stale data after TTL expired"
    );

    Ok(())
}

/// STALE READS (TTL Expiry)
/// Verifies that if a file changes on the server, the client eventually updates.
/// This tests your Attribute/Data Cache Time-To-Live (TTL).
#[test]
fn test_cache_invalidation_after_ttl() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Setup with short TTL config (e.g., 1 second)
    sys_build.client.arg_pair("--cache-ttl", "1");

    let ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("No mount"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("No server"))?;

    let file_name = "config.json";
    let contents = "v1";
    let contents_updated = "v2";

    let client_path = mount_point.join(file_name);
    let server_path = server_root.join(file_name);

    // Initial State
    fs::write(&client_path, contents)?;
    assert_eq!(fs::read_to_string(&client_path)?, contents);

    // "External" Modification (Simulate another user on the server)
    // We modify the backend file directly, bypassing the client.
    fs::write(&server_path, contents_updated)?;

    // Immediate Read (Should be HIT -> Stale v1)
    // Most FUSE clients cache for at least 1 second by default.
    let immediate = fs::read_to_string(&client_path)?;
    assert_eq!(
        immediate, contents,
        "Cache should have served stale data immediately for performance"
    );

    // Wait for TTL (e.g., > 1s)
    thread::sleep(Duration::from_secs(2));

    // Fresh Read (Should be MISS -> Fetch v2)
    let fresh = fs::read_to_string(&client_path)?;
    assert_eq!(
        fresh, contents_updated,
        "Client failed to invalidate cache after TTL expired"
    );

    Ok(())
}

/// GHOST FILES (Directory Cache)
/// Verifies that new files created on the server appear in `ls`.
#[test]
fn test_directory_listing_refresh() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Setup with short TTL config (e.g., 1 second)
    sys_build.client.arg_pair("--cache-ttl", "1");

    let ctx = sys_build.build()?;

    let file_name = "ghost.txt";
    let contents = "I am here";

    let mount_point = ctx.mount_point().ok_or(anyhow!("No mount"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("No server"))?;

    // Initial Listing
    let initial_count = fs::read_dir(&mount_point)?.count();
    assert_eq!(initial_count, 0);

    // "External" Creation
    fs::write(server_root.join(file_name), contents)?;

    // Wait for Dir Cache TTL
    thread::sleep(Duration::from_secs(2));

    // Verify Listing
    let entries: Vec<_> = fs::read_dir(&mount_point)?
        .map(|res| res.unwrap().file_name().into_string().unwrap())
        .collect();

    assert!(
        entries.contains(&file_name.to_string()),
        "New server file not showing in client directory listing"
    );

    Ok(())
}

/// METADATA PROPAGATION (Chmod)
/// Verifies that permission changes on the server reflect on the client.
#[cfg(unix)]
#[test]
fn test_remote_permission_changes() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Setup with short TTL config (e.g., 1 second)
    sys_build.client.arg_pair("--cache-ttl", "1");

    let ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("No mount"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("No server"))?;

    let filename = "script.sh";
    let client_path = mount_point.join(filename);
    let server_path = server_root.join(filename);

    fs::write(&client_path, "#!/bin/bash")?;

    // Change on Server (chmod +x)
    let mut perms = fs::metadata(&server_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&server_path, perms)?;

    // Wait for Attribute TTL
    thread::sleep(Duration::from_secs(2));

    // Verify Client sees new mode
    let new_mode = fs::metadata(&client_path)?.permissions().mode();

    // Check purely the permission bits (0o777 mask)
    assert_eq!(
        new_mode & 0o777,
        0o755,
        "Client did not pick up remote chmod"
    );

    Ok(())
}

/// WRITE VISIBILITY (Read-your-writes)
/// Verifies that if I write a file, I can read it back immediately,
/// even if the upload is async (write-back cache).
#[test]
fn test_read_your_own_writes() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();
    let file_path = mount_point.join("buffer.txt");

    let content = "This is a test string";

    // Write
    fs::write(&file_path, content)?;

    // Immediate Read (Microseconds later)
    // Even if the network is slow, the Client Cache MUST serve this from local RAM.
    let read_back = fs::read_to_string(&file_path)?;

    assert_eq!(
        read_back, content,
        "Failed to read back own write immediately"
    );

    Ok(())
}
