mod common;

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::common::*;
    use crate::setup_e2e;

    use anyhow::{Result, anyhow};
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;
    use std::fs::File;
    use std::path::Path;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    /// Helper to check if a path is currently mounted
    fn is_mounted(path: &Path) -> Result<bool> {
        // Give the system a moment to update mount table
        std::thread::sleep(Duration::from_millis(200));

        // Use the `mount` command to list active mounts
        let output = Command::new("mount")
            .output()
            .expect("Failed to run mount command");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if our path appears in the list
        Ok(stdout.contains(path.to_str().unwrap()))
    }

    /// SIGINT (Ctrl+C)
    /// Verifies that sending SIGINT causes the daemon to unmount and exit with Success (0).
    #[test]
    fn test_clean_exit_on_sigint() -> Result<()> {
        // Setup system
        let (ctx, mount_point, _server_root) = setup_e2e!();

        let mut client = ctx.client.ok_or(anyhow!("No client process"))?;
        let client_process = client.process();

        // Verify it is mounted
        assert!(
            is_mounted(&mount_point)?,
            "Filesystem should be mounted at start"
        );

        // Send SIGINT
        println!("Sending SIGINT to pid {}", client_process.id());
        signal::kill(Pid::from_raw(client_process.id() as i32), Signal::SIGINT)
            .map_err(|_| anyhow!("Failed to send SIGINT"))?;

        // Wait for process to exit
        // The context wrapper usually waits on drop, but we want to verify the exit status explicitly here.
        let exit_status = client_process.wait()?;

        // Note: Some apps exit with 0 on signal, some with 128+SIG.
        // Ideally, a graceful shutdown returns 0.
        assert!(
            exit_status.success(),
            "Daemon did not exit successfully after SIGINT"
        );

        // Verify Unmount
        assert!(
            !is_mounted(&mount_point)?,
            "Daemon exited but mount point is still active (Zombie)"
        );

        Ok(())
    }

    /// SIGTERM (Termination)
    /// Verifies that `kill <pid>` (default SIGTERM) works the same as SIGINT.
    /// This is how systemd/Docker stops services.
    #[test]
    fn test_clean_exit_on_sigterm() -> Result<()> {
        // Setup system
        let (ctx, mount_point, _server_root) = setup_e2e!();
        let mut client = ctx.client.ok_or(anyhow!("No client process"))?;
        let client_process = client.process();

        // Verify it is mounted
        assert!(
            is_mounted(&mount_point)?,
            "Filesystem should be mounted at start"
        );

        // Send SIGTERM
        signal::kill(Pid::from_raw(client_process.id() as i32), Signal::SIGTERM)
            .map_err(|_| anyhow!("Failed to send SIGTERM"))?;

        let exit_status = client_process.wait()?;
        assert!(exit_status.success());

        thread::sleep(Duration::from_millis(500));
        assert!(
            !is_mounted(&mount_point)?,
            "Daemon exited but mount point is still active (Zombie)"
        );

        Ok(())
    }

    /// SHUTDOWN WHILE BUSY
    /// Verifies behavior when the daemon is asked to stop while a file is held OPEN.
    /// A robust daemon should either:
    /// A) Force close connections and exit.
    /// B) Wait for file to close (timeout).
    /// C) Exit but lazy-unmount.
    ///
    /// We expect behavior A or C (Clean exit regardless of open files).
    #[test]
    fn test_shutdown_while_file_open() -> Result<()> {
        // Setup system
        let (ctx, mount_point, _server_root) = setup_e2e!();
        let mut client = ctx.client.ok_or(anyhow!("No client process"))?;
        let client_process = client.process();
        let file_path = mount_point.join("busy.txt");

        // Create and Open a file (Hold the handle)
        // We spawn a thread to hold the file open so we can signal the main process
        let hold_handle = thread::spawn(move || {
            let _f = File::create(&file_path).expect("Failed to create file");
            // Hold open for 5 seconds (longer than the test takes)
            thread::sleep(Duration::from_secs(5));
            // _f drops here
        });

        // Ensure the file is open
        thread::sleep(Duration::from_millis(500));

        // Send SIGINT while file is busy
        signal::kill(Pid::from_raw(client_process.id() as i32), Signal::SIGINT)
            .map_err(|_| anyhow!("Failed to send SIGINT while file is open"))?;

        // Wait for Exit
        // The daemon should NOT hang forever waiting for the file close.
        // It should force exit.
        let exit_status = client_process.wait()?;
        assert!(exit_status.success());

        // Verify Unmount
        thread::sleep(Duration::from_millis(500));
        assert!(
            !is_mounted(&mount_point)?,
            "Daemon exited but mount point is still active (Zombie)"
        );

        // Join thread (It might panic or error on file close, which is expected/ignored)
        let _ = hold_handle.join();

        Ok(())
    }
}
