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

use client::error::ClientError;
use fuse3::MountOptions;
use fuse3::path::prelude::*;
use tokio::signal;

use client::fuse::Fs;
use client::{config, error, logging, network};

type Result<T> = std::result::Result<T, error::ClientError>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Load configuration from args/env
    let config = config::Config::from_args()?;

    // Initialize logging based on config
    let _log = logging::Logging::from(&config)?;

    tracing::trace!("[TRACE]");
    tracing::debug!("[DEBUG]");
    tracing::info!("[INFO]");
    tracing::warn!("[WARN]");
    tracing::error!("[ERROR]");

    let base_url = config.server_url.clone() + network::APP_V1_BASE_URL;

    let mut mount_options = MountOptions::default();
    mount_options.allow_other(true);

    // Mount fs
    let mut mount_handle = Session::new(mount_options)
        // .mount_with_unprivileged(Fs::new(&base_url), &config.mountpoint)
        .mount(Fs::new(&base_url), &config.mountpoint)
        .await
        .map_err(|op| ClientError::Daemon(error::DaemonError::StartFailed(op.to_string())))?;

    tracing::info!("FS mounted in {:?}", config.mountpoint);

    let handle = &mut mount_handle;

    tokio::select! {
        res = handle => res.map_err(|op| ClientError::Daemon(error::DaemonError::SignalError(op.to_string())))?,
        _ = signal::ctrl_c() => {
            tracing::info!("Unmounting FS...");
            mount_handle.unmount().await.map_err(|op| ClientError::Daemon(error::DaemonError::SignalError(op.to_string())))?;
        }
    };

    println!("Done");

    Ok(())
}
