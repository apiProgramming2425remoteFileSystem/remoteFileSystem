mod common;
use common::*;

use anyhow::{Result, anyhow};
use std::fs::{self, OpenOptions};
use std::io::Write;

#[test]
fn test_data_survives_remount() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("No mount point"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("No server root"))?;

    let test_file = mount_point.join("persistent_data.txt");
    let content = "This data must survive a crash.";

    // Write Data
    fs::write(&test_file, content)?;

    // Verify it reached the server physically (Black box peek)
    let server_file = server_root.join("persistent_data.txt");
    assert!(
        server_file.exists(),
        "File did not reach server storage immediately"
    );

    // KILL Client (Simulate Crash/Shutdown)
    ctx.stop_client();

    // Remount (Start a new client against the SAME server)
    ctx.remount_client(sys_build_clone)?;

    // Verify Read on new mount
    let read_content = fs::read_to_string(&test_file)?;
    assert_eq!(
        read_content, content,
        "Data corrupted or lost after remount"
    );

    Ok(())
}

/// **Scenario:**
/// Mount the filesystem.
/// Write a file.
/// Verify the file appears in the Server's physical storage (Black Box check).
/// KILL the client process (simulating a crash or power loss).
/// Remount the filesystem.
/// Read the file to ensure data integrity.
#[test]
fn test_data_survives_client_crash_and_remount() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("Server root not found"))?;

    let test_file = mount_point.join("crash_test.data");
    let content = "This data must survive the crash.";

    // Write Data
    println!("Writing data to {:?}", test_file);
    fs::write(&test_file, content)?;

    // Verify Server Persistence (Immediate Check)
    // We bypass the FUSE client and look directly at the server's backend folder.
    // This ensures data left the client's write buffer.
    let server_file_path = server_root.join("crash_test.data");
    assert!(
        server_file_path.exists(),
        "File did not reach server storage before crash"
    );

    let server_content = fs::read_to_string(&server_file_path)?;
    assert_eq!(server_content, content, "Server received corrupted data");

    // KILL Client (Simulate Crash)
    println!("Killing client process...");
    ctx.stop_client();

    // Ensure the mount is gone (optional check)
    assert!(
        fs::metadata(&test_file).is_err(),
        "Mount should be inaccessible after kill"
    );

    // Remount
    println!("Remounting client...");
    ctx.remount_client(sys_build_clone)?;

    // Verify Read on New Mount
    let new_mount_point = ctx.mount_point().ok_or(anyhow!("Remount failed"))?;
    let survivor_file = new_mount_point.join("crash_test.data");

    let read_content = fs::read_to_string(&survivor_file)?;

    assert_eq!(read_content, content, "Data did not survive the remount!");

    println!("Test Passed: Data persisted across crash.");
    Ok(())
}

/// DATA PERSISTENCE
/// Verifies that file content survives a hard crash.
#[test]
fn test_file_content_survives_crash() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("Server root not found"))?;

    let filename = "crash_data.txt";
    let client_path = mount_point.join(filename);
    let content = "Critical data that must survive.";

    // Action: Write data
    fs::write(&client_path, content)?;

    // Verify on Server (Black Box)
    let server_path = server_root.join(filename);
    assert!(
        server_path.exists(),
        "File did not reach server storage before crash"
    );

    // CRASH & REMOUNT
    ctx.stop_client();
    ctx.remount_client(sys_build_clone)?;

    // Verify
    let new_mount = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let recovered_content = fs::read_to_string(new_mount.join(filename))?;
    assert_eq!(recovered_content, content);

    Ok(())
}

/// DIRECTORY STRUCTURE
/// Verifies that deep directory trees are reconstructed correctly.
#[test]
fn test_directory_tree_survives_crash() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("Server root not found"))?;

    // Action: Create deep structure "a/b/c/d"
    let deep_path = mount_point.join("a/b/c/d");
    fs::create_dir_all(&deep_path)?;

    // Verify on Server
    let server_deep_path = server_root.join("a/b/c/d");
    assert!(
        server_deep_path.exists(),
        "Deep directory structure not persisted to server"
    );

    // CRASH & REMOUNT
    ctx.stop_client();
    ctx.remount_client(sys_build_clone)?;

    // Verify presence
    let new_mount = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    assert!(
        new_mount.join("a/b/c/d").exists(),
        "Deep directory structure lost"
    );
    assert!(
        new_mount.join("a/b").is_dir(),
        "Intermediate path is not a directory"
    );

    Ok(())
}

/// DELETION PERSISTENCE
/// Verifies that deleted files stay deleted (no "Zombie Files").
#[test]
fn test_deletion_persists() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("Server root not found"))?;

    let filename = "to_be_deleted.txt";

    // Setup: Create file first
    fs::write(mount_point.join(filename), "temp")?;
    let server_path = server_root.join(filename);
    assert!(
        server_path.exists(),
        "File did not reach server storage before deletion"
    );

    // Action: Delete file
    fs::remove_file(mount_point.join(filename))?;

    // CRASH & REMOUNT
    ctx.stop_client();
    ctx.remount_client(sys_build_clone)?;

    // Verify: File should NOT exist
    let new_mount = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    assert!(
        !new_mount.join(filename).exists(),
        "Deleted file reappeared (Zombie)!"
    );

    Ok(())
}

/// RENAME ATOMICITY
/// Verifies that moves are durable (Old name gone, new name exists).
#[test]
fn test_rename_survives_crash() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let server_root = ctx.server_root().ok_or(anyhow!("Server root not found"))?;

    let old = mount_point.join("old_name.txt");
    let new = mount_point.join("new_name.txt");

    fs::write(&old, "move me")?;

    // Action: Rename
    fs::rename(&old, &new)?;

    // Verify Server state before crash
    assert!(
        !server_root.join("old_name.txt").exists(),
        "Old file still exists on server"
    );

    // CRASH & REMOUNT
    ctx.stop_client();
    ctx.remount_client(sys_build_clone)?;

    // Verify
    let new_mount = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    assert!(new_mount.join("new_name.txt").exists(), "New file missing");
    assert!(
        !new_mount.join("old_name.txt").exists(),
        "Old file resurrected"
    );

    Ok(())
}

/// APPEND MODE
/// Verifies that appending to a file updates the size correctly and preserves old data.
#[test]
fn test_append_persistence() -> Result<()> {
    // Setup system
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;

    // Clone for remount
    let sys_build_clone = sys_build.clone();
    let mut ctx = sys_build.build()?;

    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let file = mount_point.join("log.txt");

    // Step 1: Write initial data
    fs::write(&file, "Line 1\n")?;

    // Step 2: Open in Append mode and write more
    {
        let mut f = OpenOptions::new().append(true).open(&file)?;
        f.write_all(b"Line 2\n")?;
    } // Flush happens on drop

    // CRASH & REMOUNT
    ctx.stop_client();
    ctx.remount_client(sys_build_clone)?;

    // Verify
    let content = fs::read_to_string(mount_point.join("log.txt"))?;
    assert_eq!(
        content, "Line 1\nLine 2\n",
        "Append operation corrupted the file"
    );

    Ok(())
}
