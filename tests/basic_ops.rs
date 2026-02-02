use anyhow::Result;
use std::{fs, path::Path};

mod common;
use common::*;

#[test]
fn test_write_and_read_file() -> Result<()> {
    // Start the system with default configuration
    let (_ctx, mount_point, server_root) = setup_e2e!();

    let file_name = "hello.txt";
    let content = "Hello Remote Filesystem!";
    let file_path = mount_point.join(file_name);

    // Write: Client sends data to the server via FUSE
    fs::write(&file_path, content)?;

    // Read: Client fetches data from the server
    let read_content = fs::read_to_string(&file_path)?;
    let srv_read_content = fs::read_to_string(server_root.join(file_name))?;

    // Verify content consistency
    assert_eq!(content, read_content);
    assert_eq!(content, srv_read_content);

    Ok(())
}

#[test]
fn test_create_directory_structure() -> Result<()> {
    let (_ctx, mount_point, server_root) = setup_e2e!();

    // Define a nested structure: /parent/child/data.json
    let dirs = Path::new("parent").join("child");
    let file_name = Path::new("data.json");
    let contents = "{}";

    let dir_path = mount_point.join(&dirs);
    let file_path = dir_path.join(file_name);

    // Create nested directories (recursive)
    fs::create_dir_all(&dir_path)?;

    // Create a file inside the deep directory
    fs::write(&file_path, contents)?;

    // Verify structure existence
    assert!(dir_path.exists(), "Directory structure not found");
    assert!(file_path.exists(), "File not found inside directory");
    assert!(file_path.is_file());

    // Verify server-side structure
    let srv_dirs = server_root.join(&dirs);
    let srv_file = srv_dirs.join(file_name);

    assert!(srv_dirs.exists(), "Server directory structure not found");
    assert!(srv_file.exists(), "Server file not found inside directory");
    assert!(srv_file.is_file());

    Ok(())
}

#[test]
fn test_file_crud_lifecycle() -> Result<()> {
    let (_ctx, mount_point, server_root) = setup_e2e!();

    let file_name = "draft.txt";
    let file_rename = "final.txt";

    let file_a = mount_point.join(file_name);
    let file_b = mount_point.join(file_rename);

    let srv_a = server_root.join(file_name);
    let srv_b = mount_point.join(file_rename);

    // Create
    fs::write(&file_a, "Draft Content")?;
    assert!(file_a.exists());
    assert!(srv_a.exists());

    // Rename
    fs::rename(&file_a, &file_b)?;
    assert!(!file_a.exists(), "Old file should actually be gone");
    assert!(!srv_a.exists(), "Old file should actually be gone");
    assert!(file_b.exists(), "New file should exist");
    assert!(srv_b.exists(), "New file should exist");

    // Delete
    fs::remove_file(&file_b)?;
    assert!(!file_b.exists(), "File should be deleted");
    assert!(!srv_b.exists(), "File should be deleted");

    Ok(())
}
