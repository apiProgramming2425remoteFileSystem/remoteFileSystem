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

use fuse3::path::prelude::*;
use fuse3::MountOptions;
use tokio::signal;
use client::fuse::Fs;
use anyhow;
use client::{config, logging};




#[tokio::main(flavor = "current_thread")]
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

    let mount_path = "./mnt";

    // Imposta le opzioni di mount
    let mut mount_options = MountOptions::default();
    // Permette a utenti diversi dal proprietario di vedere il FS
    mount_options.allow_other(true);
    mount_options.force_readdir_plus(true); // opzionale, migliora compatibilità con ls

    // Monta il filesystem
    let mut mount_handle = Session::new(mount_options)
        .mount(Fs::default(), mount_path)
        .await?;

    println!("FS montato in {}", mount_path);

    let handle = &mut mount_handle;

    // Attende il completamento del mount o Ctrl+C
    tokio::select! {
        res = handle => res?,
        _ = signal::ctrl_c() => {
            println!("Unmounting FS...");
            mount_handle.unmount().await?;
        }
    };

    Ok(())
}