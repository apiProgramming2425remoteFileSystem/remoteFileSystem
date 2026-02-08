mod common;

#[cfg(unix)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::{Result, anyhow};
    use nix::unistd::{getgid, getuid};
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

    /// Verifies that Size and Permissions match EXACTLY,
    /// but Timestamps match within a 'tolerance_secs' window.
    fn assert_stat_with_tolerance(
        client_path: &Path,
        server_path: &Path,
        tolerance_secs: f64,
    ) -> Result<String> {
        // Specific format for easier parsing:
        // %s = Size (bytes)
        // %A = Permissions (e.g., -rw-r--r--)
        // %X = Access time (Seconds since Epoch)
        // %Y = Modify time (Seconds since Epoch)
        // %Z = Change time (Seconds since Epoch)
        let args = ["-c", "%s %A %X %Y %Z"];

        let (c_str, s_str) = compare_command_outputs("stat", &args, client_path, server_path)?;
        check_stat_with_tolerance(&c_str, &s_str, tolerance_secs)?;

        Ok(c_str)
    }

    fn check_stat_with_tolerance(c_str: &str, s_str: &str, tolerance_secs: f64) -> Result<()> {
        // Parse and Compare
        let c_parts: Vec<&str> = c_str.split_whitespace().collect();
        let s_parts: Vec<&str> = s_str.split_whitespace().collect();

        if c_parts.len() != 5 || s_parts.len() != 5 {
            return Err(anyhow!(
                "Unexpected stat output format.\nClient: {}\nServer: {}",
                c_str,
                s_str
            ));
        }

        // Exact Matches
        let size_match = c_parts[0] == s_parts[0]; // Size
        let perm_match = c_parts[1] == s_parts[1]; // Permissions

        // Fuzzy Matches (Timestamps)
        // We parse as f64 to handle potential fractional seconds
        let atime_diff = (c_parts[2].parse::<f64>()? - s_parts[2].parse::<f64>()?).abs();
        let mtime_diff = (c_parts[3].parse::<f64>()? - s_parts[3].parse::<f64>()?).abs();
        let ctime_diff = (c_parts[4].parse::<f64>()? - s_parts[4].parse::<f64>()?).abs();

        if !size_match
            || !perm_match
            || atime_diff > tolerance_secs
            || mtime_diff > tolerance_secs
            || ctime_diff > tolerance_secs
        {
            return Err(anyhow!(
                "Metadata mismatch!\
                [Size]    Client: {} vs Server: {} (Match: {})\n\
                [Perms]   Client: {} vs Server: {} (Match: {})\n\
                [Access]  Diff: {:.4}s (Tol: {}s)\n\
                [Modify]  Diff: {:.4}s (Tol: {}s)\n\
                [Change]  Diff: {:.4}s (Tol: {}s)",
                c_parts[0],
                s_parts[0],
                size_match,
                c_parts[1],
                s_parts[1],
                perm_match,
                atime_diff,
                tolerance_secs,
                mtime_diff,
                tolerance_secs,
                ctime_diff,
                tolerance_secs
            ));
        }
        Ok(())
    }

    // Verifies cmd stats files correctly on both Client and Server.
    #[test]
    fn test_cmd_stat_file() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "stat_test.txt";
        let data = "Data for stat test.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        // Setup: Create file first
        fs::write(&client_file, data)?;

        // COMPARE: stat (Size & Permissions)
        // Normalize the output to remove the differing file paths if 'stat' printed them.
        assert_stat_with_tolerance(&client_file, &server_file, 1.0)?;
        Ok(())
    }

    #[test]
    fn test_cmd_stat_directory() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "stat_test_dir";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory first
        fs::create_dir(&client_dir)?;

        // COMPARE: stat (Size & Permissions)
        // Normalize the output to remove the differing file paths if 'stat' printed them.
        assert_stat_with_tolerance(&client_dir, &server_dir, 1.0)?;
        Ok(())
    }

    // Verifies cmd disk usage on Client and Server.
    #[test]
    fn test_cmd_du_file() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "du_test_dir";
        let file_name = "file.txt";
        let data = "Data for du test.";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory and file inside
        fs::create_dir(&client_dir)?;
        fs::write(client_dir.join(file_name), data)?;

        // COMPARE: du -b (Byte size)
        let (mut c_out, mut s_out) =
            compare_command_outputs("du", ["-b"], &client_dir, &server_dir)?;
        c_out = c_out.split_whitespace().next().unwrap_or("").to_string();
        s_out = s_out.split_whitespace().next().unwrap_or("").to_string();
        assert_eq!(c_out, s_out, "Disk usage output mismatch");
        Ok(())
    }

    #[test]
    fn test_cmd_du_directory() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "du_test_dir";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        // Setup: Create directory first
        fs::create_dir(&client_dir)?;

        // COMPARE: du -b (Byte size)
        let (mut c_out, mut s_out) =
            compare_command_outputs("du", ["-b"], &client_dir, &server_dir)?;
        c_out = c_out.split_whitespace().next().unwrap_or("").to_string();
        s_out = s_out.split_whitespace().next().unwrap_or("").to_string();
        assert_eq!(c_out, s_out, "Disk usage output mismatch for directory");
        Ok(())
    }

    // Verifies cmd permissions on Client and Server.
    #[test]
    fn test_cmd_chmod_file() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "chmod_test.txt";
        let data = "Data for chmod test.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        fs::write(&client_file, data)?;

        for perm in ["644", "600", "755", "700", "444", "400", "777", "000"] {
            run_cmd("chmod", &[perm], Some(&client_file))?;

            let (c_out, s_out) =
                compare_command_outputs("stat", ["-c", "%A"], &client_file, &server_file)?;

            assert_eq!(
                c_out.trim(),
                s_out.trim(),
                "Chmod output mismatch for permission {} on file {}",
                perm,
                filename
            );
        }

        Ok(())
    }

    #[test]
    fn test_cmd_chmod_directory() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "chmod_test_dir";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        fs::create_dir(&client_dir)?;

        for perm in ["755", "750", "711", "700", "777"] {
            run_cmd("chmod", &[perm], Some(&client_dir))?;

            let (c_out, s_out) =
                compare_command_outputs("stat", ["-c", "%A"], &client_dir, &server_dir)?;

            assert_eq!(
                c_out.trim(),
                s_out.trim(),
                "Chmod output mismatch for permission {} on directory",
                perm
            );
        }

        Ok(())
    }

    // Verifies cmd ownership on Client and Server.
    #[test]
    fn test_cmd_chown_current_user_file() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "owned_file.txt";
        let data = "Data for chown test.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        fs::write(&client_file, data)?;

        // Get REAL user ID (works for root or normal user)
        let uid = getuid();
        let gid = getgid();
        let user_group = format!("{}:{}", uid, gid);

        // ACTION: chown <current>:<current> <file>
        run_cmd("chown", &[&user_group], Some(&client_file))?;

        // COMPARE
        let (c_out, s_out) =
            compare_command_outputs("stat", &["-c", "%u %g"], &client_file, &server_file)?;
        assert_eq!(c_out, s_out);

        Ok(())
    }

    #[test]
    fn test_cmd_chown_security_check_file() -> Result<()> {
        // Check if we are Root
        let uid = getuid().as_raw();
        if uid == 0 {
            println!("Skipping security check: Running as root allows chown.");
            return Ok(());
        }

        let (_ctx, mount_point, _server_root) = setup_e2e!();
        let client_file = mount_point.join("security.txt");
        let data = "Data for chmod test.";

        fs::write(&client_file, data)?;

        // Attempt to give file to Root (0:0)
        // As a normal user, this MUST fail.
        let output = Command::new("chown")
            .arg("0:0")
            .arg(&client_file)
            .output()?;

        assert!(
            !output.status.success(),
            "Security Breach: Non-root user changed file owner!"
        );

        Ok(())
    }

    #[test]
    fn test_cmd_chown_as_root_to_nobody_file() -> Result<()> {
        let uid = getuid().as_raw();
        // Only run this test if we ARE root
        if uid != 0 {
            return Ok(());
        }

        let (_ctx, mount_point, server_root) = setup_e2e!();
        let filename = "root_to_nobody.txt";
        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        let data = "Data for chown test.";
        fs::write(&client_file, data)?;

        // "nobody" usually has UID 65534 on Linux, but let's check /etc/passwd or just use ID 1000
        // A safer bet for generic testing is usually UID 1000 or 65534.
        let target_user = "1000:1000";

        // ACTION: Root gives file to UID 1000
        run_cmd("chown", &[target_user], Some(&client_file))?;

        std::thread::sleep(std::time::Duration::from_millis(200));

        // VERIFY
        let (c_out, s_out) =
            compare_command_outputs("stat", &["-c", "%u %g"], &client_file, &server_file)?;

        assert_eq!(
            c_out.trim(),
            "1000 1000",
            "Ownership did not update to 1000"
        );
        assert_eq!(c_out, s_out, "Server did not receive ownership change");

        Ok(())
    }

    #[test]
    fn test_cmd_chown_directory() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let dir_name = "owned_dir";

        let client_dir = mount_point.join(dir_name);
        let server_dir = server_root.join(dir_name);

        fs::create_dir(&client_dir)?;

        // Get REAL user ID (works for root or normal user)
        let uid = getuid();
        let gid = getgid();
        let user_group = format!("{}:{}", uid, gid);

        // ACTION: chown <current>:<current> <dir>
        run_cmd("chown", &[&user_group], Some(&client_dir))?;

        // Wait for sync
        std::thread::sleep(std::time::Duration::from_millis(200));

        // COMPARE
        let (c_out, s_out) =
            compare_command_outputs("stat", &["-c", "%u %g"], &client_dir, &server_dir)?;
        assert_eq!(c_out, s_out);

        Ok(())
    }

    // Verifies cmd updates timestamps on Client and Server.
    #[test]
    fn test_cmd_touch_timestamps_file() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "timestamp_test.txt";
        let data = "Data for timestamp test.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        fs::write(&client_file, data)?;

        // COMPARE: stat (Modification Time)
        let stat_before = assert_stat_with_tolerance(&client_file, &server_file, 1.0)?;

        // Set mtime to 2 hours ago
        let past_time = "2 hours ago";

        run_cmd("touch", &["-d", past_time], Some(&client_file))?;

        // COMPARE: stat (Modification Time)
        let stat_after = assert_stat_with_tolerance(&client_file, &server_file, 1.0)?;
        assert_ne!(
            stat_before, stat_after,
            "Timestamps did not update after touch command"
        );

        Ok(())
    }

    // Verifies cmd truncates files correctly on both Client and Server.
    #[test]
    fn test_cmd_truncate() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "truncate_test.txt";
        let data = "Data for truncate test.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        // Setup: Create file first
        fs::write(&client_file, data)?;

        // Truncate to 4 bytes
        run_cmd("truncate", &["-s", "4"], Some(&client_file))?;

        // COMPARE: stat (Size)
        let (c_out, s_out) =
            compare_command_outputs("stat", ["-c", "%s"], &client_file, &server_file)?;

        assert_eq!(c_out, s_out, "Truncate output mismatch");
        assert_eq!(c_out.trim(), "4", "File size after truncate is incorrect");

        Ok(())
    }
}
