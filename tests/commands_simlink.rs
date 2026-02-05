mod common;

#[cfg(unix)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::Result;
    use rstest::rstest;
    use std::fs;
    use std::os::unix::fs as unix_fs;
    use std::path::Path;
    use std::process::Command;

    // Helper to execute a command simply
    fn run_cmd(binary: &str, args: &[&str], target: Option<&Path>) -> Result<()> {
        let mut cmd = Command::new(binary);
        cmd.args(args);
        if let Some(path) = target {
            cmd.arg(path);
        }

        let status = cmd.status()?;

        assert!(
            status.success(),
            "Command {:?} {:?} {:?} failed with status: {:?}",
            binary,
            args,
            target,
            status
        );
        Ok(())
    }

    // Verifies cmd creates symlinks on Client and Server.
    #[rstest]
    #[case("symlink_target.txt", "symlink_link.txt")]
    #[case("dir_symlink_target", "dir_symlink_link")]
    fn test_cmd_symlink(#[case] target: &str, #[case] link: &str) -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let tmp_target = mount_point.join(target);
        let client_link = mount_point.join(link);
        let server_link = server_root.join(link);

        // Setup: Create target file/directory
        if link.starts_with("dir_") {
            fs::create_dir(&tmp_target)?;
        } else {
            fs::write(&tmp_target, "Symlink test data.")?;
        }

        // Create symlink using ln -s
        run_cmd(
            "ln",
            &["-s", &tmp_target.to_string_lossy()],
            Some(&client_link),
        )?;

        // Verify symlink existence on both Client and Server
        let client_metadata = fs::symlink_metadata(&client_link)?;
        let server_metadata = fs::symlink_metadata(&server_link)?;

        assert!(
            client_metadata.file_type().is_symlink(),
            "Client link is not a symlink"
        );
        assert!(
            server_metadata.file_type().is_symlink(),
            "Server link is not a symlink"
        );

        // Verify symlink points to correct target
        let client_link_target = fs::read_link(&client_link)?;
        let server_link_target = fs::read_link(&server_link)?;

        assert_eq!(
            client_link_target, tmp_target,
            "Client symlink does not point to correct target"
        );
        assert_eq!(
            server_link_target, tmp_target,
            "Server symlink does not point to correct target"
        );

        Ok(())
    }


    /// Verifies cmd fails to create symlink if target does not exist.
    // ... funziona quando replico il comportamento
    #[test]
    fn test_cmd_symlink_broken_target() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let target_file = mount_point.join("non_existent_target.txt");
        let link_file = mount_point.join("broken_link.txt");

        // ln -s SUCCEEDS when creating broken symlinks (POSIX)
        let output = Command::new("ln")
            .arg("-s")
            .arg(&target_file)
            .arg(&link_file)
            .output()?;

        // Command succeeds
        assert!(
            output.status.success(),
            "ln failed to create broken symlink!"
        );

        // Symlink exists
        assert!(link_file.exists(), "Broken symlink was not created!");

        // Target does not exist (it's broken)
        assert!(!target_file.exists(), "Target should not exist");

        // Points to correct target
        assert_eq!(
            fs::read_link(&link_file)?,
            target_file,
            "Symlink does not point to correct target"
        );

        Ok(())
    }

    /*
    // Verifies cmd creates hard links correctly.
    #[test]
    fn test_cmd_hardlink() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let target_file = mount_point.join("hardlink_target.txt");
        let link_file = mount_point.join("hardlink_link.txt");
        let server_link_file = server_root.join("hardlink_link.txt");

        // Setup: Create target file
        fs::write(&target_file, "Hardlink test data.")?;

        // Create hard link using ln
        run_cmd("ln", &[&target_file.to_string_lossy()], Some(&link_file))?;

        // Verify hard link existence on both Client and Server
        let client_metadata = fs::metadata(&link_file)?;
        let server_metadata = fs::metadata(&server_link_file)?;

        assert_eq!(
            client_metadata.ino(),
            fs::metadata(&target_file)?.ino(),
            "Client hard link does not point to the same inode as target"
        );
        assert_eq!(
            server_metadata.ino(),
            fs::metadata(&target_file)?.ino(),
            "Server hard link does not point to the same inode as target"
        );

        Ok(())
    }
    */

    // Verifies cmd reads symlink targets correctly.
    #[test]
    fn test_cmd_readlink() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let target_file = mount_point.join("readlink_target.txt");
        let link_file = mount_point.join("readlink_link.txt");

        // Setup: Create target file and symlink
        fs::write(&target_file, "Readlink test data.")?;
        unix_fs::symlink(&target_file, &link_file)?;

        // Readlink using readlink command
        let output = Command::new("readlink").arg(&link_file).output()?;

        assert!(
            output.status.success(),
            "readlink command failed with status: {:?}",
            output.status
        );

        let link_target = String::from_utf8_lossy(&output.stdout).trim().to_string();

        assert_eq!(
            link_target,
            target_file.to_str().unwrap(),
            "readlink did not return correct target"
        );

        // Negative Test: readlink on non-symlink should fail
        let output_non_symlink = Command::new("readlink").arg(&target_file).output()?;

        assert!(
            !output_non_symlink.status.success(),
            "readlink succeeded on non-symlink file!"
        );

        let res = fs::read_link(&target_file);
        assert!(res.is_err(), "fs::read_link succeeded on non-symlink file!");

        Ok(())
    }

    // Verifies cmd removes symlinks correctly.
    // ... funziona quando replico il comportamento
    #[test]
    fn test_cmd_unlink() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let target_file = mount_point.join("unlink_target.txt");
        let link_file = mount_point.join("unlink_link.txt");
        let server_link_file = server_root.join("unlink_link.txt");

        // Setup: Create target file and symlink
        fs::write(&target_file, "Unlink test data.")?;
        unix_fs::symlink(&target_file, &link_file)?;

        // Unlink using unlink command
        run_cmd("unlink", &[], Some(&link_file))?;

        // Verify symlink removal on Client
        assert!(
            !link_file.exists(),
            "Symlink still exists on Client after unlink"
        );

        // Verify symlink removal on Server
        assert!(
            !server_link_file.exists(),
            "Symlink still exists on Server after unlink"
        );

        Ok(())
    }

    // Verifies cmd fails to remove non-empty directories.
    #[test]
    fn test_cmd_unlink_non_empty_dir_fails() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let dir_name = "unlink_non_empty_dir";
        let file_name = "inside.txt";

        let dir_path = mount_point.join(dir_name);
        let file_path = dir_path.join(file_name);

        // Setup non-empty directory
        fs::create_dir(&dir_path)?;
        fs::write(&file_path, "Some data")?;

        // Try unlink on non-empty dir (Should FAIL)
        let output = Command::new("unlink").arg(&dir_path).output()?;

        assert!(
            !output.status.success(),
            "unlink succeeded on non-empty directory!"
        );

        // Verify directory still exists
        assert!(dir_path.exists(), "Directory was deleted unexpectedly");

        Ok(())
    }
}
