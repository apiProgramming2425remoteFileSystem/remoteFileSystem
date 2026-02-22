pub mod app;
pub mod cache;
pub mod commands;
pub mod config;

pub mod daemon;
pub mod error;
pub mod fs_model;
pub mod fuse;

pub mod gui;
pub mod logging;
pub mod mount;
pub mod network;

pub mod rw_buffer;
mod util;

use gui::Gui;
use rpassword::read_password;
#[cfg(unix)]
use std::fs;
use std::io::{self, Write};
use std::sync::Arc;
#[cfg(windows)]
use tokio::runtime::Runtime;

use crate::config::RfsConfig;
use crate::daemon::Daemon;
use crate::error::RfsClientError;
use crate::fuse::Fs;
use crate::mount::{MountOptions, MountPoint};
use crate::network::RemoteStorage;

type Result<T> = std::result::Result<T, RfsClientError>;

const MAX_LOGIN_ATTEMPTS: u8 = 3;

/// Runs the program with the given configuration (`config`).<br>
/// Mounts the FUSE filesystem at the given mount point and connects to the server URL.
///
/// Starts the daemon in background, unless the option `--foreground` is set.
///
/// ## Arguments
/// - `config`: Configuration for the daemon. For configuration options, see [`RfsConfig`][crate::config::RfsConfig].
/// - `rc`: An implementation of the [`RemoteStorage`] trait, which defines the interface for interacting with the remote server.
/// ### Returns
/// - `Ok(())`: if the execution was successful.
/// - `Err(_)`: if an error occurred during execution. Returns [`RfsClientError`][crate::error::RfsClientError].
///
pub fn start<R: RemoteStorage>(config: &RfsConfig, rc: R) -> Result<()> {
    println!("Starting RemoteFS...");

    if !config.gui_enabled {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow::format_err!("Failed to build temp runtime: {}", e))?
            .block_on(async {
                println!("Checking connection to server at {}...", config.server_url);
                rc.health_check().await.map_err(|err| {
                    println!("Connection to server failed: {}", err);
                    err
                })?;

                perform_login(&rc, config).await
            })?;
    }

    let rc = Arc::new(rc);

    // Instantiate the daemon
    let daemon = Daemon::new().foreground(config.foreground);
    // Initialize the daemon
    daemon.initialize()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config.logging)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    if !config.foreground {
        tracing::debug!("Background process started. PID: {}", std::process::id());
    }

    // Start the daemon
    let runtime = daemon.create_runtime()?;

    if config.gui_enabled {
        Gui::new(rc, config.clone(), daemon, runtime.clone())?.start_gui()?;
    } else {
        runtime.block_on(async {
            // Spawn the signal handler (Kill/Ctrl+C)
            daemon.spawn_signal_handler();

            run_async(
                config,
                rc,
                &daemon,
                #[cfg(windows)]
                runtime.clone(),
            )
            .await
        })?;
    }

    tracing::info!("RemoteFS execution finished.");
    Ok(())
}

async fn perform_login<R: RemoteStorage>(rc: &R, config: &RfsConfig) -> Result<String> {
    println!("Welcome to Remote File System. First you need to authenticate!");

    for i in 0..MAX_LOGIN_ATTEMPTS {
        let username = if let Some(username) = &config.username {
            println!("Using provided username: {}", username);
            username.clone()
        } else {
            println!("Username:");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        };

        let password = match std::env::var("RFS__PASSWORD") {
            Ok(pwd) => pwd,
            Err(_) => {
                println!("Password: ");
                io::stdout().flush().unwrap();
                // Hide password
                read_password().unwrap()
            }
        };

        match rc.login(username, password).await {
            Ok(token) => {
                println!("Login successful");
                return Ok(token);
            }
            Err(e) => {
                eprintln!("Login failed: {}", e);
                println!("Invalid credentials");
                println!("Attempt {}/{} to login.", i + 1, MAX_LOGIN_ATTEMPTS);
            }
        };
    }

    Err(RfsClientError::Other(anyhow::anyhow!(
        "Impossible to login: Invalid credentials!"
    )))
}

pub async fn run_async<R: RemoteStorage>(
    config: &RfsConfig,
    rc: Arc<R>,
    daemon: &Daemon,
    #[cfg(windows)] rt: Arc<Runtime>,
) -> Result<()> {
    // Create mount point directory if it doesn't exist
    #[cfg(unix)]
    if !config.mount_point.exists() {
        println!(
            "Mount point directory {:?} does not exist. Creating it.",
            config.mount_point
        );
        fs::create_dir_all(&config.mount_point).map_err(|err| {
            RfsClientError::Other(anyhow::format_err!(
                "Could not create mount point directory: {}",
                err
            ))
        })?;
    }

    // Create Filesystem
    let fs = Fs::new(
        rc,
        config,
        #[cfg(windows)]
        rt.clone(),
    );

    let mount_options = MountOptions::from(&config.mount);

    let mut mount_point = MountPoint::new(&config.mount_point, mount_options);

    // Mount fs
    mount_point.mount(fs).await.map_err(|err| {
        tracing::error!("MOUNT ERROR: {}", err); // Write to log
        eprintln!("MOUNT ERROR: {}", err); // Write to daemon.err
        err
    })?;

    tokio::select! {
        // Ends when the mount session ends
        res = mount_point.wait() => {
            match res {
                Ok(_) => tracing::info!("Mount session ended normally"),
                Err(e) => {
                    tracing::error!("Mount session ended with error: {}", e);
                    return Err(RfsClientError::Daemon(error::DaemonError::SignalError(format!("mount session error: {}", e))));
                }
            }
        }
        // Ends when a shutdown signal is received
        _ = daemon.wait_for_shutdown() => {
            tracing::info!("Shutdown signal received via Daemon.");

            match mount_point.unmount().await {
                Ok(_) => tracing::info!("Unmounted successfully on shutdown."),
                Err(_) => {
                    tracing::error!("Error during unmount on shutdown. Attempting lazy unmount.");
                    // Attempt lazy unmount, exiting immediately.
                    match  mount_point.lazy_unmount().await {
                        Ok(_) => tracing::info!("Lazy unmount successful."),
                        Err(e) => tracing::error!("Error during lazy unmount: {}", e)
                    }
                }
            }
        }
    }

    tracing::info!("Cleanup complete. Exiting.");
    Ok(())
}
