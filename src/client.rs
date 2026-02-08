use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::binary::{BinaryBuilder, get_bin};
use crate::{DEFAULT_PASS, DEFAULT_USER, LogStrategy, apply_logging};

// Cache the path of the client binary
static CLIENT_BIN: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug)]
pub struct ClientProcess {
    child: Child,
    pub mount_point: PathBuf,
}

impl ClientProcess {
    pub fn spawn(
        builder: BinaryBuilder,
        mount_point: &Path,
        server_url: &str,
        log_strategy: &LogStrategy,
    ) -> Result<Self> {
        fs::create_dir_all(mount_point)?;

        let bin = CLIENT_BIN.get_or_init(|| {
            println!("Building Client binary...");
            get_bin("client")
        });

        let mut cmd = Command::new(bin);

        cmd.arg("run");
        cmd.arg("--mount-point").arg(mount_point);
        cmd.arg("--server-url").arg(server_url);
        cmd.arg("--foreground");
        cmd.arg("--username").arg(DEFAULT_USER);
        cmd.env("RFS__PASSWORD", DEFAULT_PASS);

        builder.apply_to(&mut cmd);
        apply_logging(&mut cmd, log_strategy, "client");

        match cmd.spawn() {
            Ok(child) => Ok(Self {
                child,
                mount_point: mount_point.to_path_buf(),
            }),
            Err(e) => Err(anyhow!("Failed to spawn Client process: {}", e)),
        }
    }

    pub fn process(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn wait_ready(&mut self, wait_time: Duration) -> Result<()> {
        let start = Instant::now();
        let pool_interval = Duration::from_millis(50);

        #[cfg(unix)]
        // Parent device ID. Used to detect mount status.
        let parent_dev = {
            use std::os::unix::fs::MetadataExt;

            let parent = self
                .mount_point
                .parent()
                .ok_or_else(|| anyhow!("Mount point has no parent directory"))?;
            fs::metadata(parent)
                .map_err(|e| anyhow!("Failed to read parent directory metadata: {}", e))?
                .dev()
        };

        while start.elapsed() < wait_time {
            // Check if process is still alive
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(anyhow!(
                    "Client process exited early with status: {}",
                    status
                ));
            }

            // Check if mount point is effectively mounted
            if self.is_mounted(
                #[cfg(unix)]
                parent_dev,
            ) {
                println!(
                    "Client mounted at {:?} in {:?}",
                    self.mount_point,
                    start.elapsed()
                );
                return Ok(());
            }

            std::thread::sleep(pool_interval);
        }

        Err(anyhow!(
            "Timeout waiting for mount at {:?} after {:?}",
            self.mount_point,
            wait_time
        ))
    }

    fn is_mounted(&self, #[cfg(unix)] parent_dev: u64) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            // If we can't read metadata, it's definitely not ready/mounted yet
            if let Ok(meta) = fs::metadata(&self.mount_point) {
                // On Unix, a mount point has a different Device ID than its parent
                return meta.dev() != parent_dev;
            }

            false
        }

        // TODO: Implement for Windows if needed
        #[cfg(windows)]
        {
            // Placeholder: If metadata is readable, assume mounted
            fs::metadata(&self.mount_point).is_ok()
        }

        #[cfg(not(any(unix, windows)))]
        false
    }
}

impl Drop for ClientProcess {
    fn drop(&mut self) {
        let Ok(exit_status) = self.child.try_wait() else {
            return;
        };

        if exit_status.is_some() {
            return;
        }

        if let Err(e) = self.child.kill() {
            eprintln!("warning: failed to kill client process: {}", e);
        }
        if let Err(e) = self.child.wait() {
            eprintln!("warning: failed to wait on client process: {}", e);
        }

        // Logic to unmount gracefully the FUSE filesystem
        /*
        #[cfg(unix)]
        Command::new("umount")
            .arg(&self.mount_point)
            .status()
            .expect("Failed to unmount");
        */
        #[cfg(unix)]
        {
            Command::new("fusermount")
                .arg("-u")
                .arg("-z")
                .arg(&self.mount_point)
                .status()
                .expect("Failed to unmount");
        }
    }
}
