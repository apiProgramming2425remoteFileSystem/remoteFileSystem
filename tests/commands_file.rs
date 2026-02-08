mod common;

#[cfg(unix)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::{Result, anyhow};
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

    fn check_stat_with_tolerance(
        c_str: &str,
        s_str: &str,
        tolerance_secs: f64,
    ) -> Result<()> {
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
            [Change]  Diff: {:.4}s (Tol: {}s)\n",
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

    // Verifies that `touch` creates a new file correctly on the mounted filesystem.
    #[test]
    fn test_cmd_touch_creation() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "new_touch.txt";
        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        run_cmd("touch", &[], Some(&client_file))?;

        // COMPARE: stat (Size & Permissions)
        // Normalize the output to remove the differing file paths if 'stat' printed them.
        assert_stat_with_tolerance(&client_file, &server_file, 1.0)?;
        Ok(())
    }

    // Verifies cmd reads the same content from both Client and Server.
    #[rstest]
    #[case("cat", &[] as &[&str])]
    #[case("head", &["-n","5"])]
    #[case("head", &["-n", "5"])]
    #[case("tail", &["-n", "5"])]
    fn test_cmd_read(#[case] cmd: &str, #[case] args: &[&str]) -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "read.txt";
        let data = "Hello from echo!";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        fs::write(&client_file, data)?;

        // COMPARE: cat
        // Content should be identical
        let (c_out, s_out) = compare_command_outputs(cmd, args, &client_file, &server_file)?;
        assert_eq!(c_out, s_out);

        Ok(())
    }

    // Verifies cmd writes files correctly on both Client and Server.
    #[test]
    fn test_cmd_write() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "write.txt";
        let data = "Data written via echo command.";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        // Write using echo
        run_cmd(
            "sh",
            &[
                "-c",
                &format!("echo '{}' > {}", data, client_file.to_string_lossy()),
            ],
            None,
        )?;

        // COMPARE: cat
        let (c_out, s_out) =
            compare_command_outputs("cat", &[] as &[&str], &client_file, &server_file)?;
        assert_eq!(c_out.trim(), s_out.trim(), "Written content mismatch");

        Ok(())
    }

    // Verifies cmd appends files correctly on both Client and Server.
    #[test]
    fn test_cmd_append() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "append.txt";
        let initial_data = "Initial line.\n";
        let append_data = "Appended line.\n";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        // Initial write
        fs::write(&client_file, initial_data)?;

        // Append using echo
        run_cmd(
            "sh",
            &[
                "-c",
                &format!(
                    "echo '{}' >> {}",
                    append_data.trim(),
                    client_file.to_string_lossy()
                ),
            ],
            None,
        )?;

        // COMPARE: cat
        let (c_out, s_out) =
            compare_command_outputs("cat", &[] as &[&str], &client_file, &server_file)?;
        assert_eq!(c_out, s_out, "Appended content mismatch");

        Ok(())
    }

    // Verifies cmd integrity for contents of files on Client and Server.
    #[rstest]
    #[case("sha512sum", 1, "c")]
    #[case("sha512sum", 2, "c")]
    #[case("sha512sum", 4, "c")]
    #[case("sha512sum", 200, "c")]
    #[case("sha512sum", 1, "K")]
    #[case("sha512sum", 2, "K")]
    #[case("sha512sum", 4, "K")]
    #[case("sha512sum", 200, "K")]
    #[case("sha512sum", 1, "M")]
    #[case("sha512sum", 2, "M")]
    #[case("sha512sum", 4, "M")]
    #[case("sha512sum", 200, "M")]
    fn test_cmd_hash_integrity(
        #[case] cmd: &str,
        #[case] size: usize,
        #[case] dim: &str, // c=1, w=2, b=512, K=1024, M=1024*1024, G=1024*1024*1024
    ) -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "hash.bin";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);
        let tmp_file = server_root
            .join("temp_hash_data.bin")
            .to_string_lossy()
            .to_string();

        run_cmd(
            "dd",
            &[
                "if=/dev/urandom",
                &format!("of={}", tmp_file),
                &format!("bs={}", dim),
                &format!("count={}", size),
                "status=none",
            ],
            None,
        )?;

        println!("Generated random file of size {} {}", size, dim);

        // Copy to Client Mount
        run_cmd("cp", &[&tmp_file], Some(&client_file))?;
        assert!(
            client_file.exists(),
            "Client file not found after copy file {} {}",
            size,
            dim
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            server_file.exists(),
            "Server file not found after copy file {} {}",
            size,
            dim
        );

        // Diff
        let (c_out, s_out) =
            compare_command_outputs("diff", [&tmp_file], &client_file, &server_file)?;
        assert_eq!(
            c_out, s_out,
            "Stat output mismatch for size {} {}",
            size, dim
        );

        // COMPARE: stat (Size & Permissions)
        // Normalize the output to remove the differing file paths if 'stat' printed them.
        assert_stat_with_tolerance(&client_file, &server_file, 1.0).map_err(|e| {
            anyhow!(
                "Stat metadata mismatch between Client and Server for size {} {}: {}",
                size,
                dim,
                e
            )
        })?;

        // COMPARE: hash outputs
        let (c_out, s_out) =
            compare_command_outputs(cmd, &[] as &[&str], &client_file, &server_file)?;
        assert_eq!(
            c_out.split_whitespace().next(),
            s_out.split_whitespace().next(),
            "Hash mismatch between Client and Server for size {} {}",
            size,
            dim
        );

        Ok(())
    }

    // Verifies cmd renames files correctly on both Client and Server.
    #[test]
    fn test_cmd_rename() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let original_name = "original.txt";
        let renamed_name = "renamed.txt";
        let data = "Data for rename test.";

        let client_original = mount_point.join(original_name);
        let client_renamed = mount_point.join(renamed_name);
        let server_original = server_root.join(original_name);
        let server_renamed = server_root.join(renamed_name);

        // Setup: Create original file
        fs::write(&client_original, data)?;
        assert!(
            server_original.exists(),
            "File did not reach server storage before rename"
        );

        // Rename using mv
        run_cmd(
            "mv",
            &[&client_original.to_string_lossy()],
            Some(&client_renamed),
        )?;

        // Verify renaming on both Client and Server
        assert!(
            !client_original.exists(),
            "Original file still exists on Client after rename"
        );
        assert!(
            !server_original.exists(),
            "Original file still exists on Server after rename"
        );
        assert!(
            client_renamed.exists(),
            "Renamed file does not exist on Client after rename"
        );
        assert!(
            server_renamed.exists(),
            "Renamed file does not exist on Server after rename"
        );

        // Verify content integrity
        let client_content = fs::read_to_string(&client_renamed)?;
        let server_content = fs::read_to_string(&server_renamed)?;
        assert_eq!(client_content, data, "Client renamed file content mismatch");
        assert_eq!(server_content, data, "Server renamed file content mismatch");

        Ok(())
    }

    // Verifies cmd removes files correctly on both Client and Server.
    #[test]
    fn test_cmd_remove() -> Result<()> {
        let (_ctx, mount_point, server_root) = setup_e2e!();

        let filename = "to_be_removed.txt";

        let client_file = mount_point.join(filename);
        let server_file = server_root.join(filename);

        // Setup: Create file first
        fs::write(&client_file, "temporary data")?;
        assert!(
            server_file.exists(),
            "File did not reach server storage before deletion"
        );

        // Remove using rm
        run_cmd("rm", &[], Some(&client_file))?;

        // Verify deletion on both Client and Server
        assert!(
            !client_file.exists(),
            "File still exists on Client after deletion"
        );
        assert!(
            !server_file.exists(),
            "File still exists on Server after deletion"
        );

        Ok(())
    }
}
