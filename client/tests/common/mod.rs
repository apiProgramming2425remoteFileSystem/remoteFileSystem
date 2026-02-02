use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_test::assert_ok;

use client::config::logging::LogTargets;
use client::config::{LoggingConfig, RfsConfig};
use client::daemon::Daemon;
use client::logging::Logging;
use client::network::MockRemoteStorage;
use client::run_async;

pub const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Install a global panic hook that forcefully unmounts the FUSE filesystem in case of a panic.
/// Call this function as the FIRST thing in your test.
pub fn register_fuse_panic_hook(mount_path: PathBuf) {
    // Get the original hook to preserve default panic behavior
    let original_hook = std::panic::take_hook();

    // Set a new hook that wraps the original and adds unmount logic
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("GLOBAL PANIC DETECTED inside test process.");
        // Call the original hook to print the panic info
        original_hook(info);

        // Perform lazy unmount to avoid deadlocks
        #[cfg(unix)]
        {
            eprintln!("Attempting to unmount the FUSE filesystem...");
            let _ = std::process::Command::new("fusermount")
                .arg("-u")
                .arg("-z")
                .arg(&mount_path)
                .spawn(); // Fire & Forget (do not block the panic handler)
        }
    }));
}

pub struct AppController {
    // pub mount_dir: TempDir,
    _logger: Logging,
    daemon: Daemon,
    app_handle: JoinHandle<()>,
}

impl AppController {
    /// Starts the client application with the provided configuration and mock storage.
    pub async fn start(config: RfsConfig, mock: MockRemoteStorage) -> Result<Self> {
        register_fuse_panic_hook(config.mount_point.to_path_buf());

        // Initialize logging based on config
        let _logger = Logging::from(&config.logging)?;

        // Initialize the daemon in foreground mode to avoid forking the process
        let daemon = Daemon::new().foreground(true);
        daemon.initialize()?;

        // Clone the daemon handle for the app task
        let daemon_handle = daemon.clone();

        // Spawn the core application logic in a separate Tokio task.
        // This allows the test logic to run concurrently in the main thread.
        let app_handle = tokio::spawn(async move {
            // Start the client with the mock
            run_async(config, mock, daemon_handle)
                .await
                .expect("Failed to run async client");

            println!("Application task has exited.");
        });

        // Wait for the client to start
        wait_ready(Duration::from_millis(50)).await?;
        Ok(Self {
            _logger,
            daemon,
            app_handle,
        })
    }

    /// Runs the provided future with a timeout to detect potential deadlocks.
    pub async fn run_with_timeout<T, F>(&self, future: F) -> Result<T>
    where
        F: Future<Output = T>,
    {
        tokio::time::timeout(TEST_TIMEOUT, future)
            .await
            .map_err(|_| {
                anyhow!(
                    "Operation timed out after {:?}. Possible FUSE deadlock or zombie process.",
                    TEST_TIMEOUT
                )
            })
    }

    /// Shuts down the client application and waits for the task to complete.
    pub async fn shutdown(self) -> Result<()> {
        self.daemon.trigger_shutdown();

        assert_ok!(self.app_handle.await);
        Ok(())
    }
}

async fn wait_ready(wait_time: Duration) -> Result<()> {
    tokio::time::sleep(wait_time).await;
    Ok(())
}

pub fn get_config(mountpoint: &Path) -> RfsConfig {
    RfsConfig {
        mount_point: mountpoint.to_path_buf(),
        username: Some("test_user".to_string()),
        logging: LoggingConfig {
            log_targets: vec![LogTargets::Console],
            ..Default::default()
        },
        ..Default::default()
    }
}
