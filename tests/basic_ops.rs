use anyhow::{Result, anyhow};
use std::fs;

mod common;
use common::*;

#[test]
fn test_write_and_read_file() -> Result<()> {
    // Start the system with default configuration
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;
    let ctx = sys_build.build()?;

    let file_path = ctx
        .mount_point()
        .ok_or_else(|| anyhow!("Client context missing"))?
        .join("hello.txt");

    let content = "Hello Remote Filesystem!";

    // Write: Client sends data to the server via FUSE
    fs::write(&file_path, content).expect("Failed to write file");

    // Read: Client fetches data from the server
    let read_content = fs::read_to_string(&file_path).expect("Failed to read file");

    // Verify content consistency
    assert_eq!(content, read_content);

    Ok(())
}

#[test]
fn test_create_directory_structure() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;
    let ctx = sys_build.build()?;

    // Define a nested structure: /parent/child/data.json
    let dir_path = ctx
        .mount_point()
        .ok_or_else(|| anyhow!("Client context missing"))?
        .join("parent/child");

    let file_path = dir_path.join("data.json");

    // Create nested directories (recursive)
    fs::create_dir_all(&dir_path)?;

    // Create a file inside the deep directory
    fs::write(&file_path, "{}")?;

    // Verify structure existence
    assert!(dir_path.exists(), "Directory structure not found");
    assert!(file_path.exists(), "File not found inside directory");
    assert!(file_path.is_file());

    Ok(())
}

#[test]
fn test_file_crud_lifecycle() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;
    let ctx = sys_build.build()?;

    let mount_dir = ctx
        .mount_point()
        .ok_or_else(|| anyhow!("Client context missing"))?;
    let file_a = mount_dir.join("draft.txt");
    let file_b = mount_dir.join("final.txt");

    // Create
    fs::write(&file_a, "Draft Content")?;
    assert!(file_a.exists());

    // Rename
    fs::rename(&file_a, &file_b)?;
    assert!(!file_a.exists(), "Old file should actually be gone");
    assert!(file_b.exists(), "New file should exist");

    // Delete
    fs::remove_file(&file_b)?;
    assert!(!file_b.exists(), "File should be deleted");

    Ok(())
}
