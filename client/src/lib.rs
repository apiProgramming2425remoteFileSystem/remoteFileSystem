pub mod cache;
pub mod config;
pub mod daemon;
pub mod error;
pub mod fs_model;
pub mod fuse;
pub mod logging;
pub mod mount;
pub mod network;

pub mod rw_buffer;

use std::fs;
use std::io::{self, Write};

use anyhow;
use rpassword::read_password;
use tracing;

use crate::config::Config;
use crate::daemon::Daemon;
use crate::error::ClientError;
use crate::fuse::Fs;
use crate::mount::{MountOptions, MountPoint};
use crate::network::RemoteClient;

type Result<T> = std::result::Result<T, ClientError>;

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

    let rc = RemoteClient::new(&config.server_url);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            ClientError::Other(anyhow::format_err!(
                "Failed to build Tokio runtime: {}",
                err
            ))
        })?;

    runtime.block_on(async {
        println!("Checking connection to server at {}...", config.server_url);

        rc.health_check().await.map_err(|err| {
            println!("Connection to server failed: {}", err); // Write to log
            err
        })?;

        rc.health_check().await?;

        println!("Welcome to Remote File System. First you need to authenticate!");

        let token_option = loop {
            println!("username:");
            let username = {
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                input.trim().to_string()
            };
            println!("Password: ");
            io::stdout().flush().unwrap();
            // Hide password
            let password = read_password().unwrap();

            match rc.login(username, password).await {
                Ok(t) => break Some(t),
                Err(e) => {
                    eprintln!("Login failed: {}", e);
                    println!("Invalid credentials. Do you want to try again? [y n]");
                    let mut answer = String::new();
                    io::stdin().read_line(&mut answer).unwrap();
                    if !answer.trim().to_string().starts_with("y") {
                        break None;
                    }
                }
            };
        };

        if token_option.is_none() {
            return Err(ClientError::Other(anyhow::anyhow!(
                "Impossible to login: Invalid credentials!"
            )));
        }

        // let token = token_option.unwrap();
        println!("Login successful");
        Ok(())
    })?;

    drop(runtime);

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
    daemon.create_runtime(run_async(config.clone(), rc, daemon.clone()))?;

    tracing::info!("RemoteFS execution finished.");
    Ok(())
}

async fn run_async(config: Config, rc: RemoteClient, daemon: Daemon) -> Result<()> {
    /*
    tracing::info!("Checking connection to server at {}...", config.server_url);

    rc.health_check().await.map_err(|err| {
        tracing::error!("Connection to server failed: {}", err); // Write to log
        err
    })?;
    */

    let cache_config = config.cache_config();
    let xattributes_enabled = config.xattributes_enabled;

    // Create Filesystem
    let fs = Fs::new(rc, cache_config, xattributes_enabled);

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
