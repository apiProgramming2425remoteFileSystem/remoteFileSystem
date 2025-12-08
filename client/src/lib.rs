pub mod config;
pub mod daemon;
pub mod error;
pub mod fs_model;
pub mod fuse;
pub mod logging;
pub mod mount;
pub mod network;

use std::fs;
use tracing;

use crate::config::Config;
use crate::daemon::Daemon;
use crate::error::ClientError;
use crate::fuse::Fs;
use crate::mount::{MountOptions, MountPoint};

type Result<T> = std::result::Result<T, ClientError>;

/// Runs the program with the given configuration ([`config`]).<br>
/// Mounts the FUSE filesystem at the given mountpoint and connects to the server URL.
///
/// Starts the daemon in background, unless the option `--foreground` is set.
///
/// ## Arguments
/// - [`config`]: Configuration for the daemon. For configuration options, see [`Config`][crate::config::Config].
/// ### Returns
/// - `Ok(())`: if the execution was successful.
/// - `Err(_)`: if an error occurred during execution. Returns [`ClientError`][crate::error::ClientError].
///
pub fn start(config: &Config) -> Result<()> {
    println!("Starting RemoteFS...");

    // Create mountpoint directory if it doesn't exist
    if !config.mountpoint.exists() {
        println!(
            "Mountpoint directory {:?} does not exist. Creating it.",
            config.mountpoint
        );
        fs::create_dir_all(&config.mountpoint).map_err(|err| {
            ClientError::Other(
                anyhow::format_err!("Could not create mountpoint directory: {}", err).into(),
            )
        })?;
    }

    // Instantiate the daemon
    let daemon = Daemon::new().foreground(config.foreground);
    // Initialize the daemon
    daemon.initialize()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    tracing::debug!("Background process started. PID: {}", std::process::id());

    // Start the daemon
    daemon.create_runtime(run_async(config.clone(), daemon.clone()))?;

    tracing::info!("RemoteFS execution finished.");
    Ok(())
}

async fn run_async(config: Config, daemon: Daemon) -> Result<()> {
    // TODO: check the connection to the server before mounting, retry if necessary in case of failure

    // Create Filesystem
    let fs = Fs::new(&config.server_url);

    let mount_options = MountOptions::from(&config);

    let mut mountpoint = MountPoint::new(&config.mountpoint, mount_options);

    // Mount fs
    mountpoint.mount(fs).await.map_err(|err| {
        tracing::error!("MOUNT ERROR: {}", err); // Write to log
        eprintln!("MOUNT ERROR: {}", err); // Write to daemon.err
        err
    })?;

    tokio::select! {
        // Ends when the mount session ends
        res = mountpoint.wait() => {
            match res {
                Ok(_) => tracing::info!("Mount session ended normally"),
                Err(e) => {
                    tracing::error!("Mount session ended with error: {}", e);
                    return Err(ClientError::Daemon(error::DaemonError::SignalError(format!("mount session error: {}", e))));
                }
            }
        }
        // Ends when a shutdown signal is received
        _ = daemon.wait_for_shutdown() => {
            tracing::info!("Shutdown signal received via Daemon.");
            // Procediamo all'unmount pulito
            if let Err(e) = mountpoint.unmount().await {
                tracing::error!("Error during graceful unmount: {}", e);
            }
        }
    };

    tracing::info!("Cleanup complete. Exiting.");
    Ok(())
}
