use anyhow::{Result, anyhow};
use std::fs;

mod common;
use common::setup;

#[test]
fn test_special_characters_in_filenames() -> Result<()> {
    let ctx = setup()?.build()?;

    // List of tricky filenames
    let filenames = vec![
        "file with spaces.txt",
        "pass@word!.log",
        "caffè_latte.txt", // Unicode characters
        "[bracketed].doc",
        "-start-dash.txt",
    ];

    for name in filenames {
        let path = ctx
            .mount_point()
            .ok_or_else(|| anyhow!("Client context missing"))?
            .join(name);
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
    let ctx = setup()?.build()?;

    // Generate a 5MB payload
    // Ensure this doesn't exceed your cache size if testing with strict limits
    let size = 5 * 1024 * 1024;
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
    let ctx = setup()?.build()?;
    let mut current_path = ctx
        .mount_point()
        .expect("Client context missing")
        .to_path_buf();

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
