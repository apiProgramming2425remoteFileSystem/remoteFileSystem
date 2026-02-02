use anyhow::{Result, anyhow};
use std::fs;

mod common;
use common::*;

#[test]
fn test_special_characters_in_filenames() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    // List of tricky filenames
    let filenames = vec![
        "file with spaces.txt",
        "pass@word!.log",
        "caffè_latte.txt", // Unicode characters
        "[bracketed].doc",
        "-start-dash.txt",
    ];

    for name in filenames {
        let path = mount_point.join(name);
        let content = format!("Content of {}", name);

        // Write
        fs::write(&path, &content)?;
        // Assert existence
        assert!(path.exists(), "Failed to handle filename: {}", name);

        // Read back
        let read = fs::read_to_string(&path)?;
        assert_eq!(read, content);
    }

    Ok(())
}

#[test]
fn test_large_file_transfer() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Generate a 5MB payload
    // Ensure this doesn't exceed your cache size if testing with strict limits
    let size = 5 * 1024 * 1024;
    sys_build
        .client
        .arg_pair("--cache-max-size", &size.to_string());

    let ctx = sys_build.build()?;

    let large_data = vec![65u8; size]; // 5MB of 'A's

    let path = ctx
        .mount_point()
        .ok_or_else(|| anyhow!("Client context missing"))?
        .join("big_image.iso");

    // Write large file
    fs::write(&path, &large_data)?;

    // Read large file
    let read_data = fs::read(&path)?;

    // Verify size and content integrity
    assert_eq!(read_data.len(), size, "File size mismatch");
    assert_eq!(read_data, large_data, "Content mismatch");

    Ok(())
}

#[test]
fn test_directory_depth_limit() -> Result<()> {
    // Tests if the system handles deep recursion gracefully
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let mut current_path = mount_point.to_path_buf();

    // Create a path 10 levels deep
    for i in 0..10 {
        current_path.push(format!("level_{}", i));
    }

    // Create the deep structure
    fs::create_dir_all(&current_path)?;

    // Write a file at the bottom
    let file_path = current_path.join("bottom.txt");
    fs::write(&file_path, "I am deep")?;
    assert!(file_path.exists());

    // Read back
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "I am deep");

    Ok(())
}

#[test]
fn test_complex_tree_manipulation() -> Result<()> {
    let (_ctx, root, _server_root) = setup_e2e!();

    // Deep Nesting
    let deep_path = root.join("a/b/c/d/e/f/g");
    fs::create_dir_all(&deep_path)?;
    assert!(deep_path.exists());

    // Special Characters
    let weird_name = deep_path.join("file with spaces & symbols!.txt");
    fs::write(&weird_name, "content")?;
    assert!(weird_name.exists());

    // Directory Rename (The hardest test for FUSE clients)
    // Move 'a' to 'z'. 'z/b/c...' must still exist.
    let new_root = root.join("z");
    fs::rename(root.join("a"), &new_root)?;

    assert!(!root.join("a").exists(), "Old directory still exists");
    assert!(
        new_root.join("b/c/d/e/f/g").exists(),
        "Children not moved with parent"
    );
    assert!(
        new_root
            .join("b/c/d/e/f/g/file with spaces & symbols!.txt")
            .exists(),
        "File lost in move"
    );

    Ok(())
}
