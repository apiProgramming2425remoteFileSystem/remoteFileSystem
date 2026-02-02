mod common;

#[cfg(unix)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::Result;
    use rstest::rstest;
    use std::fs;
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

    fn normalize_ls_output(output: &str, root_path: &Path) -> String {
        output
            .lines()
            .map(|line| {
                if line.starts_with("total") {
                    return String::new();
                }
                let line = line.replace(root_path.to_str().unwrap_or(""), "");

                // show permissions, size, date, time, name only
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 2 {
                    format!("{} {}", parts[0], parts[4..].join(" "))
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // Verifies cmd creates directories on Client and Server.
    #[rstest]
    #[case(&[] as &[&str], "new_directory")]
    #[case(&["-p"], "parent_dir/nested_dir")]
    #[case(&["-m", "755"], "mode_dir")]
    #[case(&["-m", "700", "-p"], "secure/inner_dir")]
    fn test_cmd_mkdir(#[case] args: &[&str], #[case] path: &str) -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let client_dir = mount_point.join(path);
        let server_dir = server_root.join(path);

        // Create directory using mkdir
        run_cmd("mkdir", args, Some(&client_dir))?;
        // Verify existence on both Client and Server
        assert!(
            client_dir.exists() && client_dir.is_dir(),
            "Directory not found on Client after mkdir"
        );
        assert!(
            server_dir.exists() && server_dir.is_dir(),
            "Directory not found on Server after mkdir"
        );

        Ok(())
    }

    // Verifies cmd lists directories on Client and Server.
    #[test]
    fn test_cmd_ls_simple() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "ls_test_dir";
        let file_name = "file.txt";
        let data = "Data for ls test.";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory and file inside
        fs::create_dir(&client_dir)?;
        fs::write(client_dir.join(file_name), data)?;

        // COMPARE: ls -l
        let (c_out, s_out) = compare_command_outputs("ls", ["-l"], &client_dir, &server_dir)?;

        assert_eq!(
            normalize_ls_output(&c_out, &mount_point),
            normalize_ls_output(&s_out, &server_root),
            "ls output mismatch"
        );
        Ok(())
    }

    #[test]
    fn test_cmd_ls_la_hidden_files() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "ls_hidden_dir";
        let visible_file = "visible.txt";
        let hidden_file = ".hidden.txt";
        let data = "Data for ls hidden files test.";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory and files inside
        fs::create_dir(&client_dir)?;
        fs::write(client_dir.join(visible_file), data)?;
        fs::write(client_dir.join(hidden_file), data)?;

        // COMPARE: ls -la
        let (c_out, s_out) = compare_command_outputs("ls", ["-la"], &client_dir, &server_dir)?;

        assert_eq!(
            normalize_ls_output(&c_out, &mount_point),
            normalize_ls_output(&s_out, &server_root),
            "ls -la output mismatch"
        );
        Ok(())
    }

    #[test]
    fn test_cmd_ls_recursive() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "ls_recursive_dir";
        let sub_dir = "subdir";
        let file_name = "file.txt";
        let data = "Data for ls recursive test.";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory structure
        fs::create_dir_all(client_dir.join(sub_dir))?;
        fs::write(client_dir.join(sub_dir).join(file_name), data)?;

        // COMPARE: ls -R
        let (c_out, s_out) = compare_command_outputs("ls", ["-R"], &client_dir, &server_dir)?;

        assert_eq!(
            normalize_ls_output(&c_out, &mount_point),
            normalize_ls_output(&s_out, &server_root),
            "ls -R output mismatch"
        );
        Ok(())
    }

    // Verifies cmd removes directories on Client and Server.
    #[test]
    fn test_cmd_rmdir_simple() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "rmdir_test_dir";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory first
        fs::create_dir(&client_dir)?;
        assert!(
            server_dir.exists(),
            "Directory did not reach server storage before deletion"
        );

        // Remove using rmdir
        run_cmd("rmdir", &[], Some(&client_dir))?;

        // Verify deletion on both Client and Server
        assert!(
            !client_dir.exists(),
            "Directory still exists on Client after deletion"
        );
        assert!(
            !server_dir.exists(),
            "Directory still exists on Server after deletion"
        );

        Ok(())
    }

    #[test]
    fn test_cmd_rmdir_non_empty_directory() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let dir_name = "rmdir_non_empty_dir";
        let file_name = "file.txt";
        let data = "Data for rmdir non-empty test.";

        let client_dir = mount_point.join(dir_name);

        // Setup: Create directory and a file inside
        fs::create_dir(&client_dir)?;
        fs::write(client_dir.join(file_name), data)?;

        // Attempt to remove non-empty directory using rmdir
        let output = Command::new("rmdir").arg(&client_dir).output()?;

        // Verify that rmdir failed
        assert!(
            !output.status.success(),
            "rmdir succeeded on non-empty directory, which is unexpected"
        );

        // Clean up by removing the file and then the directory
        fs::remove_file(client_dir.join(file_name))?;
        fs::remove_dir(&client_dir)?;

        Ok(())
    }

    #[test]
    fn test_cmd_rmdir_non_existent_directory() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let dir_name = "rmdir_non_existent_dir";

        let client_dir = mount_point.join(dir_name);

        // Ensure the directory does not exist
        if client_dir.exists() {
            fs::remove_dir_all(&client_dir)?;
        }

        // Attempt to remove non-existent directory using rmdir
        let output = Command::new("rmdir").arg(&client_dir).output()?;

        // Verify that rmdir failed
        assert!(
            !output.status.success(),
            "rmdir succeeded on non-existent directory, which is unexpected"
        );

        Ok(())
    }

    #[test]
    fn test_cmd_rmdir_multiple_directories() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let dir_names = vec!["rmdir_dir1", "rmdir_dir2", "rmdir_dir3"];
        let mut client_dirs = Vec::new();

        // Setup: Create multiple directories
        for dir_name in &dir_names {
            let client_dir = mount_point.join(dir_name);
            fs::create_dir(&client_dir)?;
            client_dirs.push(client_dir);
        }

        // Remove multiple directories using rmdir
        let mut cmd = Command::new("rmdir");
        for dir in &client_dirs {
            cmd.arg(dir);
        }
        let output = cmd.output()?;

        // Verify that rmdir succeeded
        assert!(
            output.status.success(),
            "rmdir failed on multiple directories"
        );

        // Verify deletion on Client
        for dir in &client_dirs {
            assert!(
                !dir.exists(),
                "Directory {} still exists on Client after deletion",
                dir.to_string_lossy()
            );
        }

        Ok(())
    }

    #[test]
    fn test_cmd_rm_r() -> Result<()> {
        let (_ctx, mount_point, _server_root) = setup_e2e!();

        let dir_name = "rm_recursive_dir";
        let sub_dir = "subdir";
        let file_name = "file.txt";
        let data = "Data for rm -r test.";

        let client_dir = mount_point.join(dir_name);

        // Setup: Create directory structure
        fs::create_dir_all(client_dir.join(sub_dir))?;
        fs::write(client_dir.join(sub_dir).join(file_name), data)?;

        // Remove using rm -r
        let output = Command::new("rm").arg("-r").arg(&client_dir).output()?;

        // Verify that rm -r succeeded
        assert!(
            output.status.success(),
            "rm -r failed on recursive directory"
        );

        // Verify deletion on Client
        assert!(
            !client_dir.exists(),
            "Directory still exists on Client after rm -r deletion"
        );

        Ok(())
    }
}
