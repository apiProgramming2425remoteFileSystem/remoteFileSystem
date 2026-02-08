use std::fs::{self, OpenOptions};

use anyhow::Result;

mod common;
use common::*;

#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_attributes_and_timestamps() -> Result<()> {
        // Setup system
        let (_ctx, root, _server_root) = setup_e2e!();
        let script_path = root.join("script.sh");

        // Create File
        fs::write(&script_path, "#!/bin/bash\necho 'Hello'")?;

        // Test CHMOD (Make executable)
        let mut perms = fs::metadata(&script_path)?.permissions();

        perms.set_mode(0o755); // rwxr-xr-x
        fs::set_permissions(&script_path, perms)?;

        // Verify mode persisted
        let new_perms = fs::metadata(&script_path)?.permissions();
        let new_mode = new_perms.mode() & 0o777;
        assert_eq!(
            new_mode, 0o755,
            "Chmod did not persist correctly: {:o} expected {:o} ",
            new_mode, 0o755
        );

        // Test TOUCH (Update Timestamps)
        // Set mtime to 1 hour ago
        let past = SystemTime::now() - Duration::from_secs(3600);

        // Use `filetime` crate or shell command for utimens
        let status = Command::new("touch")
            .arg("-d")
            .arg("1 hour ago")
            .arg(&script_path)
            .status()?;
        assert!(status.success());

        // Verify mtime is roughly 1 hour ago
        let mtime = fs::metadata(&script_path)?.modified()?;
        let diff = mtime.duration_since(past).unwrap_or_default();

        assert!(diff.as_secs() < 2, "Timestamp was not updated correctly");

        Ok(())
    }

    /// PERMISSIONS (CHMOD)
    /// Verifies that `chmod` changes are stored and returned correctly.
    #[test]
    fn test_chmod_persistence() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let file_name = "script.sh";
        let file_path = mount_point.join(file_name);

        fs::write(&file_path, "#!/bin/bash\nexit 0")?;

        // Change to 755 (rwxr-xr-x)
        let mut perms = fs::metadata(&file_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&file_path, perms)?;

        // Verify Local Metadata
        let new_mode = fs::metadata(&file_path)?.permissions().mode();
        assert_eq!(new_mode & 0o777, 0o755, "Local metadata did not update");

        // Verify Server Persistence (Black Box)
        // We check the server backing file to ensure the metadata was transmitted.
        // NOTE: server mirrors the FS attributes.
        let server_file = server_root.join(file_name);
        let server_mode = fs::metadata(&server_file)?.permissions().mode();
        assert_eq!(
            server_mode & 0o777,
            0o755,
            "Server did not receive chmod command for file {}",
            file_name
        );

        Ok(())
    }

    /// TIMESTAMPS (TOUCH / UTIMENS)
    /// Verifies that modification time (mtime) can be explicitly set.
    /// Critical for build tools like `make`.
    #[test]
    fn test_mtime_updates() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();
        let file_path = mount_point.join("old_doc.txt");

        fs::write(&file_path, "data")?;

        // Set mtime to 1 hour ago
        let past = SystemTime::now() - Duration::from_secs(3600);

        // Rust's std::fs doesn't easily support setting mtime (set_times is unstable/nightly).
        // We use the `touch` command which calls `utimensat`.
        let status = Command::new("touch")
            .arg("-d")
            .arg("1 hour ago")
            .arg(&file_path)
            .status()?;
        assert!(status.success());

        // Verify
        let mtime = fs::metadata(&file_path)?.modified()?;
        let diff = mtime.duration_since(past).unwrap_or_default();

        // Allow small skew
        assert!(
            diff.as_secs() < 2,
            "mtime was not updated to the past. Diff: {}s",
            diff.as_secs()
        );

        Ok(())
    }

    /// PERMISSION ENFORCEMENT
    /// Verifies that Read-Only files cannot be written to.
    /// Note: This relies on FUSE option `default_permissions` OR your client enforcing it.
    #[test]
    fn test_readonly_enforcement() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let file_path = mount_point.join("readonly.txt");
        fs::write(&file_path, "Do not touch")?;

        // Set to 444 (r--r--r--)
        let mut perms = fs::metadata(&file_path)?.permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&file_path, perms)?;

        // Verify mode persisted
        let new_perms = fs::metadata(&file_path)?.permissions();
        assert_eq!(new_perms.mode() & 0o777, 0o444);

        // Attempt write
        {
            use std::io::Write;

            let mut f = fs::OpenOptions::new().write(true).open(&file_path)?;

            // Even if this "succeeds", it must not persist
            let _ = f.write_all(b"EVIL WRITE");
            let _ = f.flush();
        }

        // Give background thread time to process (if needed)
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Verify content unchanged
        let content = fs::read_to_string(&file_path)?;
        assert_eq!(content, "Do not touch");

        Ok(())
    }
}

/// FILE SIZE ACCURACY
/// Verifies that metadata size matches actual content, especially after truncates.
#[test]
fn test_filesize_truncate() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();
    let file_path = mount_point.join("truncate.bin");

    // Write 1KB
    let data = vec![0u8; 1024];
    fs::write(&file_path, &data)?;
    assert_eq!(fs::metadata(&file_path)?.len(), 1024);

    // Truncate Down (Shrink to 500b)
    let mut new_size = 500;
    let f = OpenOptions::new().write(true).open(&file_path)?;
    f.set_len(new_size)?;
    assert_eq!(fs::metadata(&file_path)?.len(), new_size);

    // Truncate Up (Grow to 2KB - creates hole)
    new_size = 2048;
    f.set_len(new_size)?;
    assert_eq!(fs::metadata(&file_path)?.len(), new_size);

    Ok(())
}
