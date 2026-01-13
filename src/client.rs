use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::binary::{BinaryBuilder, get_bin};
use crate::{LogStrategy, apply_logging};

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

        cmd.arg("--mountpoint").arg(mount_point);
        cmd.arg("--server-url").arg(server_url);
        cmd.arg("--username").arg("test_user");
        cmd.env("PASSWORD", "test_password");
        cmd.arg("--foreground");

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

    pub fn wait_ready(&self, wait_time: Duration) -> Result<()> {
        std::thread::sleep(wait_time);
        Ok(())
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

        // Logica unmount graceful vista prima
        #[cfg(unix)]
        Command::new("umount")
            .arg(&self.mount_point)
            .status()
            .expect("Failed to unmount");

        self.child.kill().expect("Failed to kill client process");
        self.child.wait().expect("Failed to wait on client process");
    }
}
