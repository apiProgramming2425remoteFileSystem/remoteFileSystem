/*

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration from args/env
    let config = config::Config::from_args()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    println!("Done");

    let base_url = config.server_url + network::APP_V1_BASE_URL;
    let rc = network::client::RemoteClient::new(base_url);
    rc.list_path("/").await?;

    // Start daemon and mount FUSE file system
    // daemon::run_daemon(config).await?;

    Ok(())
}


use client::fuse::SimpleFS;
use fuser;
use fuser::MountOption;
use std::process::Command;
use std::fs;


fn main() {


    // Provo a smontare se era già montato
    let _ = Command::new("fusermount")
        .args(&["-u", &mountpoint])
        .status();

    // Ricreo la cartella (se era rimasta "zombie" la elimino e la rifaccio pulita)
    let _ = fs::remove_dir_all(&mountpoint);
    let _ = fs::create_dir_all(&mountpoint);

    fuser::mount2(SimpleFS, mountpoint, &[MountOption::RO]).unwrap();
}
*/

use std::fs;

use client::network::client::RemoteClient;
use fuse3::MountOptions;
use fuse3::path::prelude::*;
use tokio::signal;

use anyhow::anyhow;
use client::error::ClientError;
use client::fuse::Fs;
use client::{config, error, logging, network};
use rpassword::read_password;
use std::io::{self, Write};

type Result<T> = std::result::Result<T, error::ClientError>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load configuration from args/env
    let config = config::Config::from_args()?;

    let base_url = config.server_url.clone() + network::APP_V1_BASE_URL;

    let rc = RemoteClient::new(&base_url);

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    // --- 1. User login ---
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
        return Err(ClientError::Other(anyhow!(
            "Impossible to login: Invalid credentials!"
        )));
    }

    let token = token_option.unwrap();

    // --- 2. File system mounting ---

    // Create mountpoint directory if it doesn't exist
    if !config.mountpoint.exists() {
        tracing::info!(
            "Mountpoint directory {:?} does not exist. Creating it.",
            config.mountpoint
        );
        fs::create_dir_all(&config.mountpoint)
            .map_err(|err| ClientError::Daemon(error::DaemonError::StartFailed(err.to_string())))?;
    }

    let mut mount_options = MountOptions::default();
    mount_options.allow_other(true);

    let cache_config = config.cache_config();

    // Mount fs
    let mut mount_handle = Session::new(mount_options)
        // .mount_with_unprivileged(Fs::new(&base_url), &config.mountpoint)
        .mount(Fs::new(&base_url, cache_config), &config.mountpoint)
        .await
        .map_err(|err| ClientError::Daemon(error::DaemonError::StartFailed(err.to_string())))?;

    tracing::info!("FS mounted in {:?}", config.mountpoint);

    let handle = &mut mount_handle;

    tokio::select! {
        res = handle => res.map_err(|err| ClientError::Daemon(error::DaemonError::SignalError(err.to_string())))?,
        _ = signal::ctrl_c() => {
            tracing::info!("Unmounting FS...");
            mount_handle.unmount().await.map_err(|err| ClientError::Daemon(error::DaemonError::SignalError(err.to_string())))?;
        }
    };

    println!("Done");

    Ok(())
}
