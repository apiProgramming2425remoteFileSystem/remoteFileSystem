use anyhow::{Result, anyhow};
use std::fs;
use std::thread;
use std::time::Duration;

mod common;
use common::*;

#[test]
fn test_correct_lifecycle() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    let ctx = sys_build.build()?;

    let Some(mount_dir) = ctx.mount_point() else {
        return Err(anyhow!("Client context missing"));
    };

    println!("Using mount point at {:?}", mount_dir);
    std::fs::write(mount_dir.join("file.txt"), "content")?;

    let Some(server) = &ctx.server else {
        return Err(anyhow!("Server root missing"));
    };

    assert!(server.fs_root.join("file.txt").exists());
    println!("Test completed, resources will be cleaned up on drop.");
    Ok(())
}

#[test]
fn test_data_propagation_to_server_disk() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;
    let ctx = sys_build.build()?;

    let filename = "integration_check.txt";
    let content = "Data traveling through the pipeline";

    let mount_dir = &ctx.client.expect("Client context missing").mount_point;

    // Client Write (Entry point: FUSE)
    let client_file = mount_dir.join(filename);
    fs::write(&client_file, content).expect("Failed to write to FUSE mount");

    // Wait for propagation (Network latency + Server IO)
    // Even if local, give it a tiny moment if architecture is async
    thread::sleep(Duration::from_millis(100));

    // Server Verification (End point: Server Disk)
    // We bypass the network and look directly at the server's temp folder
    let Some(server) = ctx.server else {
        return Err(anyhow!("Server context missing"));
    };

    let server_file = server.fs_root.join(filename);

    assert!(server_file.exists(), "File did not reach server disk!");

    let server_content = fs::read_to_string(&server_file)?;
    assert_eq!(server_content, content, "Content corrupted during transfer");

    Ok(())
}

#[test]
fn test_cache_serves_content_when_server_is_dead() -> Result<()> {
    // Setup with explicit Caching Enabled
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    let cache_ttl_seconds = 3;

    sys_build
        .client
        .arg_pair("--cache-ttl", &cache_ttl_seconds.to_string());

    let mut ctx = sys_build.build()?;

    let file_path = ctx
        .mount_point()
        .ok_or_else(|| anyhow!("Client context missing"))?
        .join("cached_doc.txt");

    let content = "Persisted in RAM";

    // Create file (Write-through to server usually)
    fs::write(&file_path, content)?;

    // Prime the cache (First Read -> Fetches from Server -> Stores in Cache)
    fs::read_to_string(&file_path)?;

    ctx.stop_server();

    // Read again
    // IF Cache works: Returns content immediately.
    // IF Cache fails: Tries to contact server -> Connection Refused -> Test Fails.
    let cached_content = fs::read_to_string(&file_path);

    assert!(
        cached_content.is_ok(),
        "Read failed! Cache did not intervene."
    );
    assert_eq!(
        cached_content.expect("Failed to read cached content"),
        content
    );

    thread::sleep(Duration::from_secs(cache_ttl_seconds + 1));

    let post_ttl_read = fs::read_to_string(&file_path);
    assert!(
        post_ttl_read.is_err(),
        "Read succeeded after TTL expiry! Cache did not expire."
    );

    Ok(())
}

#[test]
fn test_client_sees_existing_files_on_startup() -> Result<()> {
    // Prepare the SERVER STATE separately first
    // We start a server, put a file in its root, then start the client later.
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;
    sys_build.no_client();

    let ctx_srv = sys_build.build()?;

    let file_name = "ancient_scroll.txt";

    // Inject a file directly into Server Root (Simulating existing data)
    let existing_file = ctx_srv
        .server_root()
        .ok_or_else(|| anyhow!("Server context missing"))?
        .join(file_name);

    fs::write(&existing_file, "Old Wisdom")?;

    // Now Start the Client MANUALLY
    // We attach it to the existing server context
    let mut sys_build = test_env.setup()?;
    sys_build.no_server().host("localhost").port(8080); // Match the server host and port

    let ctx_clt = sys_build.build()?;

    let Some(mount_dir) = ctx_clt.mount_point() else {
        return Err(anyhow!("Client context missing"));
    };

    // Verify Client sees the file via FUSE
    let client_view_path = mount_dir.join(file_name);

    assert!(
        client_view_path.exists(),
        "Client did not fetch metadata list on startup"
    );
    assert_eq!(
        fs::read_to_string(client_view_path).expect("Failed to read client view path"),
        "Old Wisdom"
    );

    Ok(())
}

#[test]
fn test_data_is_persisted_on_server_disk() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;
    let ctx = sys_build.build()?;

    // Path on the Client (Virtual FUSE mount)
    let Some(clt_mnt) = ctx.mount_point() else {
        return Err(anyhow!("Client context missing"));
    };

    let client_path = clt_mnt.join("payload.bin");

    // Path on the Server (Actual physical temp dir)
    let Some(srv_root) = ctx.server_root() else {
        return Err(anyhow!("Server context missing"));
    };

    let server_path = srv_root.join("payload.bin");

    let data = vec![0u8; 1024]; // 1KB of dummy data

    // Client writes the file
    fs::write(&client_path, &data)?;

    // Wait for eventual consistency (network lag, write buffers)
    // Reduce this sleep if your server is strictly synchronous
    thread::sleep(Duration::from_millis(100));

    // Verification: Does the file exist on the server's actual disk?
    assert!(
        server_path.exists(),
        "Server failed to persist file to disk!"
    );

    // Verification: Is the content binary identical?
    let server_data = fs::read(server_path)?;
    assert_eq!(data, server_data, "Data corruption detected on server disk");

    Ok(())
}
