mod common;

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::{Result, anyhow};
    use std::fs;
    use std::io::ErrorKind;
    use std::os::unix::fs as unix_fs; // Unix-specific extension

    #[test]
    fn test_symlink_behavior() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let original_file = mount_point.join("original.txt");
        let link_file = mount_point.join("link_to_original");

        // Create Source File
        fs::write(&original_file, "Target Content")?;

        // Create Symlink (ln -s original.txt link_to_original)
        // Note: FUSE must implement `symlink` opcode for this to work
        unix_fs::symlink(&original_file, &link_file)?;

        // Verify Link Resolution (Follow the link)
        let content = fs::read_to_string(&link_file)?;
        assert_eq!(content, "Target Content");

        // Verify Metadata
        let meta = fs::symlink_metadata(&link_file)?;
        assert!(
            meta.file_type().is_symlink(),
            "File reported as regular file, not symlink"
        );

        // Break the Link (Delete source)
        fs::remove_file(&original_file)?;

        // Verify Broken Link Behavior
        // Reading a broken link should fail with NotFound
        let res = fs::read_to_string(&link_file);
        assert!(res.is_err());

        Ok(())
    }

    /// BASIC SYMLINK (File)
    /// Verifies `ln -s target link` works and reading the link returns target data.
    #[test]
    fn test_symlink_creation_and_read() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let target = mount_point.join("original.txt");
        let link = mount_point.join("alias.txt");

        // Create Target
        fs::write(&target, "Hello World")?;

        // Create Symlink (link -> original.txt)
        unix_fs::symlink(&target, &link).map_err(|e| anyhow!("Failed to create symlink: {}", e))?;

        // Verify Metadata
        let meta = fs::symlink_metadata(&link)?;
        assert!(
            meta.file_type().is_symlink(),
            "File reported as regular file, expected symlink"
        );

        // Verify Read (Follows link)
        let content = fs::read_to_string(&link)?;
        assert_eq!(content, "Hello World");

        Ok(())
    }

    /// DIRECTORY SYMLINK
    /// Verifies `ln -s dir link_dir` works and we can `ls` inside it.
    #[test]
    fn test_symlink_to_directory() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let real_dir = mount_point.join("real_folder");
        let link_dir = mount_point.join("link_folder");

        // Setup Dir structure
        fs::create_dir(&real_dir)?;
        fs::write(real_dir.join("inside.txt"), "Secret Data")?;

        // Create Symlink
        unix_fs::symlink(&real_dir, &link_dir)?;

        // Read through link
        let linked_file = link_dir.join("inside.txt");
        assert!(linked_file.exists());

        let content = fs::read_to_string(linked_file)?;
        assert_eq!(content, "Secret Data");

        Ok(())
    }

    /// BROKEN SYMLINKS (Dangling)
    /// Verifies behavior when the target is deleted.
    #[test]
    fn test_broken_symlink_behavior() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let target = mount_point.join("temp.txt");
        let link = mount_point.join("broken.lnk");

        fs::write(&target, "Temporary")?;
        unix_fs::symlink(&target, &link)?;

        // Delete Target
        fs::remove_file(&target)?;

        // Verify Link still exists (as an inode)
        assert!(fs::symlink_metadata(&link).is_ok());

        // Verify Read fails (NotFound)
        let err = fs::read_to_string(&link).unwrap_err();
        assert_eq!(
            err.kind(),
            ErrorKind::NotFound,
            "Reading broken link should return NotFound"
        );

        Ok(())
    }

    /// SYMLINK CHAIN (A -> B -> C)
    /// Verifies that the OS/Client can follow multiple hops.
    #[test]
    fn test_symlink_chain_resolution() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let file_c = mount_point.join("c.txt");
        let link_b = mount_point.join("b.lnk"); // Points to c
        let link_a = mount_point.join("a.lnk"); // Points to b

        fs::write(&file_c, "Final Destination")?;

        unix_fs::symlink(&file_c, &link_b)?;
        unix_fs::symlink(&link_b, &link_a)?;

        let content = fs::read_to_string(&link_a)?;
        assert_eq!(content, "Final Destination");

        Ok(())
    }
}
