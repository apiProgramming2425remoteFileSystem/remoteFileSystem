pub mod app;
pub mod cache;
pub mod commands;
pub mod config;

#[cfg(unix)]
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
use std::fs;
use std::io::{self, Write};
use std::ops::Deref;

use rpassword::read_password;

use crate::config::RfsConfig;
#[cfg(unix)]
use crate::daemon::Daemon;
use crate::error::RfsClientError;
use crate::fuse::Fs;
#[cfg(windows)]
use crate::mount::windows::mount_windows;
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
/// - `config`: Configuration for the daemon. For configuration options, see [`Config`][crate::config::Config].
/// ### Returns
/// - `Ok(())`: if the execution was successful.
/// - `Err(_)`: if an error occurred during execution. Returns [`ClientError`][crate::error::ClientError].
///
#[cfg(unix)]
pub fn start_unix<R: RemoteStorage + Clone>(config: &RfsConfig, rc: R) -> Result<()> {
    println!("Starting RemoteFS...");

    if config.no_gui {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                RfsClientError::Other(anyhow::format_err!(
                    "Failed to build Tokio runtime: {}",
                    err
                ))
            })?;

        // Check on server health and user login
        runtime.block_on(async {
            println!("Checking connection to server at {}...", config.server_url);

            rc.health_check().await.map_err(|err| {
                println!("Connection to server failed: {}", err); // Write to log
                err
            })?;

            rc.health_check().await?;

            perform_login(&rc, config).await
        })?;

        drop(runtime);

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

        tracing::debug!("Background process started. PID: {}", std::process::id());

        // Start the daemon
        daemon.create_runtime(run_async(config.clone(), rc, daemon.clone()))?;

        tracing::info!("RemoteFS execution finished.");
        Ok(())
    } else {
        // Initialize logging based on config
        let _log = logging::Logging::from(&config.logging)?;

        tracing::trace!("[TRACE]");
        tracing::debug!("[DEBUG]");
        tracing::info!("[INFO]");
        tracing::warn!("[WARN]");
        tracing::error!("[ERROR]");

        let config_var: RfsConfig = config.to_owned();
        Gui::new(rc, config_var)?.start_gui()?;
        Ok(())
    }
}

#[cfg(windows)]
fn start_windows<R: RemoteStorage>(config: &RfsConfig, rc: R) -> Result<()> {
    println!("Starting RemoteFS (Windows / WinFSP)");

    let _log = logging::Logging::from(&config.logging)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            RfsClientError::Other(anyhow::format_err!(
                "Failed to build Tokio runtime: {}",
                err
            ))
        })?;
    runtime.block_on(async {
        rc.health_check().await?;
        perform_login(&rc, config).await?;
        Ok::<_, RfsClientError>(())
    })?;

    drop(runtime);

    mount_windows(rc, config)?;

    Ok(())
}

/// Runs the program with the given configuration (`config`).<br>
/// Mounts the FUSE filesystem at the given mountpoint and connects to the server URL.
///
/// Starts the daemon in background, unless the option `--foreground` is set.
///
/// ## Arguments
/// - `config`: Configuration for the daemon. For configuration options, see [`Config`][crate::config::Config].
/// ### Returns
/// - `Ok(())`: if the execution was successful.
/// - `Err(_)`: if an error occurred during execution. Returns [`ClientError`][crate::error::ClientError].
///
pub fn start<R: RemoteStorage + Clone>(config: &RfsConfig, rc: R) -> Result<()> {
    #[cfg(unix)]
    {
        start_unix(config, rc)
    }
    #[cfg(windows)]
    {
        start_windows(config, rc)
    }
}

async fn perform_login<R: RemoteStorage + Clone>(rc: &R, config: &RfsConfig) -> Result<String> {
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

#[cfg(unix)]
pub async fn run_async<R: RemoteStorage + Clone>(config: RfsConfig, rc: R, daemon: Daemon) -> Result<()> {
    /*
    tracing::info!("Checking connection to server at {}...", config.server_url);

    rc.health_check().await.map_err(|err| {
        tracing::error!("Connection to server failed: {}", err); // Write to log
        err
    })?;
    */

    // Create mountpoint directory if it doesn't exist
    if !config.mount_point.exists() {
        println!(
            "Mountpoint directory {:?} does not exist. Creating it.",
            config.mount_point
        );
        fs::create_dir_all(&config.mount_point).map_err(|err| {
            RfsClientError::Other(
                anyhow::format_err!("Could not create mountpoint directory: {}", err).into(),
            )
        })?;
    }

    // Create Filesystem
    let fs = Fs::new(rc, &config);

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
            // Attempt lazy unmount, exiting immediately.
            match mount_point.lazy_unmount().await {
                Ok(()) => tracing::info!("Requested lazy unmount (detach)"),
                Err(e) => {
                    // Fallback to graceful unmount
                    tracing::warn!("Lazy unmount failed: {}. Falling back to graceful unmount.", e);
                    if let Err(e) = mount_point.unmount().await {
                        tracing::error!("Error during graceful unmount: {}", e);
                    }
                }
            }
        }
    };

    tracing::info!("Cleanup complete. Exiting.");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dummy() {
        assert_eq!(1 + 1, 2);
    }
}
