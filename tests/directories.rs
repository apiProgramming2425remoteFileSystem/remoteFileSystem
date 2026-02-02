use anyhow::Result;
use std::fs;
use std::io::ErrorKind;

mod common;
use common::*;

#[test]
fn test_directory_safety_rules() -> Result<()> {
    let (_ctx, root, _server_root) = setup_e2e!();

    let dir_name = "parent";
    let dir_rename = "target_dir";
    let file_name = "child.txt";
    let file_name2 = "ignore.txt";
    let contents = "data";

    let dir = root.join(dir_name);
    let child = dir.join(file_name);

    // Setup non-empty directory
    fs::create_dir(&dir)?;
    fs::write(&child, contents)?;

    // Try RMDIR on non-empty dir (Should FAIL)
    let err = fs::remove_dir(&dir).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::DirectoryNotEmpty,
        "Allowed deleting non-empty directory!"
    );

    // Try RENAME Overwrite
    // Rename 'A' to 'B', where 'B' is a non-empty directory (Should FAIL)
    let dir_b = root.join(dir_rename);
    fs::create_dir(&dir_b)?;
    fs::write(dir_b.join(file_name2), contents)?;

    let err_rename = fs::rename(&dir, &dir_b).unwrap_err();
    // POSIX says EEXIST or ENOTEMPTY
    assert!(matches!(
        err_rename.kind(),
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty
    ));

    Ok(())
}

/// NON-EMPTY DELETION SAFETY
/// Verifies that `rmdir` (remove_dir) fails if the directory is not empty.
/// This prevents accidental data loss.
#[test]
fn test_rmdir_non_empty_fails() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let parent = mount_point.join("parent");
    let child = parent.join("child.txt");

    // Setup: Create dir and file inside
    fs::create_dir(&parent)?;
    fs::write(&child, "data")?;

    // Attempt RMDIR (Should Fail)
    let result = fs::remove_dir(&parent);

    assert!(result.is_err(), "Allowed deleting a non-empty directory!");

    let err = result.unwrap_err();
    // POSIX Error: ENOTEMPTY (Mapped to DirectoryNotEmpty in Rust)
    assert_eq!(err.kind(), ErrorKind::DirectoryNotEmpty);

    // Verify files still exist
    assert!(child.exists());

    Ok(())
}

/// RENAME OVERWRITE (Directory -> Empty Directory)
/// POSIX allows renaming 'A' over 'B' IF 'B' is an empty directory.
#[test]
fn test_rename_overwrite_empty_dir() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let src = mount_point.join("source_dir");
    let dest = mount_point.join("dest_dir");

    // Setup: Two directories
    fs::create_dir(&src)?;
    fs::write(src.join("file.txt"), "content")?; // Src has content

    fs::create_dir(&dest)?; // Dest is empty

    // Attempt Rename (Should Succeed)
    // "dest_dir" should be removed and replaced by "source_dir"
    fs::rename(&src, &dest)?;

    // Verify
    assert!(!src.exists());
    assert!(dest.exists());
    assert!(
        dest.join("file.txt").exists(),
        "Content was not moved to destination"
    );

    Ok(())
}

/// RENAME OVERWRITE FAILURE (Directory -> Non-Empty Directory)
/// POSIX forbids renaming 'A' over 'B' if 'B' has content.
#[test]
fn test_rename_overwrite_non_empty_fails() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let src = mount_point.join("source");
    let dest = mount_point.join("dest");

    fs::create_dir(&src)?;
    fs::create_dir(&dest)?;

    // Dest has a file!
    fs::write(dest.join("blocker.txt"), "I block the move")?;

    // Attempt Rename (Should Fail)
    let result = fs::rename(&src, &dest);

    assert!(
        result.is_err(),
        "Allowed overwriting a non-empty directory!"
    );
    let err = result.unwrap_err();
    assert!(matches!(
        err.kind(),
        ErrorKind::DirectoryNotEmpty | ErrorKind::AlreadyExists
    ));

    Ok(())
}

/// DEEP NESTING LIMITS
/// Verifies the system handles creating and reading very deep paths (e.g., 20+ levels).
/// This tests path buffer limits and recursion in your logic.
#[test]
fn test_deeply_nested_directories() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let mut current_path = mount_point.to_path_buf();
    let depth = 20;

    // Create Deep Structure iteratively
    for i in 0..depth {
        current_path.push(format!("level_{}", i));
        fs::create_dir(&current_path)?;
    }

    // Write file at bottom
    let file_path = current_path.join("bottom.txt");
    fs::write(&file_path, "Success")?;

    // Verify Existence
    assert!(file_path.exists());
    let content = fs::read_to_string(file_path)?;
    assert_eq!(content, "Success");

    // Recursive Delete (fs::remove_dir_all)
    // This issues many 'readdir' and 'unlink' calls
    let first_level = mount_point.join("level_0");
    fs::remove_dir_all(&first_level)?;

    assert!(!first_level.exists());

    Ok(())
}
