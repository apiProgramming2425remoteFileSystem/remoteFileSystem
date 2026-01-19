use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::binary::{BinaryBuilder, get_bin};
use crate::{LogStrategy, apply_logging};

// Cache the path of the server binary
static SERVER_BIN: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug)]
pub struct ServerProcess {
    child: Child,
    pub host: String,
    pub port: u16,
    pub fs_root: PathBuf,
}

impl ServerProcess {
    pub fn spawn(
        builder: BinaryBuilder,
        host: &str,
        port: u16,
        fs_root: &Path,
        log_strategy: &LogStrategy,
    ) -> Result<Self> {
        std::fs::create_dir_all(fs_root)?;

        let bin = SERVER_BIN.get_or_init(|| {
            println!("Building Server binary...");
            get_bin("server")
        });

        let mut cmd = Command::new(bin);

        cmd.arg("--server-host").arg(host);
        cmd.arg("--port").arg(port.to_string());
        cmd.arg("--filesystem-root").arg(fs_root);

        builder.apply_to(&mut cmd);
        apply_logging(&mut cmd, log_strategy, "server");

        match cmd.spawn() {
            Ok(child) => Ok(Self {
                child,
                host: host.to_string(),
                port,
                fs_root: fs_root.to_path_buf(),
            }),
            Err(e) => Err(anyhow!("Failed to spawn Server process: {}", e)),
        }
    }

    pub fn wait_ready(&self, wait_time: Duration) -> Result<()> {
        let address = format!("{}:{}", self.host, self.port);
        let deadline = Instant::now() + wait_time;

        while Instant::now() < deadline {
            if std::net::TcpStream::connect(&address).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Err(anyhow!(
            "Server did not start listening on {} within {} seconds",
            address,
            wait_time.as_secs()
        ))
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.child.kill().expect("Failed to kill server process");
        self.child.wait().expect("Failed to wait on server process");
    }
}
