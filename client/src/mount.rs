#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::MountError;
use crate::fuse::Fs;

use async_trait::async_trait;
use tracing::{Level, instrument};

type Result<T> = std::result::Result<T, MountError>;

/// Mountpoint representation
pub struct MountPoint {
    mountpoint: PathBuf,
    options: MountOptions,
    session: Box<dyn MountFs>,
}

/// Mount configuration
#[derive(Debug)]
pub struct MountOptions {
    read_only: bool,
    allow_other: bool,
    unprivileged: bool,
}

/// Trait for mounting and unmounting the filesystem.
#[async_trait]
pub trait MountFs: Send + Sync {
    /// Mounts the filesystem at the specified mountpoint.
    #[allow(unused_variables)]
    async fn mount(&mut self, fs: Fs, mountpoint: &Path, options: &MountOptions) -> Result<()> {
        Err(MountError::UnsupportedPlatform(
            "Mounting not supported on this platform".into(),
        ))
    }

    /// Waits for the mount session to end.
    async fn wait(&mut self) -> Result<()> {
        Err(MountError::UnsupportedPlatform(
            "Waiting for mount session not supported on this platform".into(),
        ))
    }

    /// Unmounts the filesystem from the mountpoint.
    async fn unmount(&mut self) -> Result<()> {
        Err(MountError::UnsupportedPlatform(
            "Unmounting not supported on this platform".into(),
        ))
    }
}

impl MountPoint {
    pub fn new<P: AsRef<Path>>(mountpoint: P, options: MountOptions) -> Self {
        let driver = create_driver();

        Self {
            mountpoint: mountpoint.as_ref().to_path_buf(),
            options,
            session: driver,
        }
    }

    pub fn mountpoint(&self) -> &Path {
        &self.mountpoint
    }
    pub fn options(&self) -> &MountOptions {
        &self.options
    }
    pub fn session(&self) -> &Box<dyn MountFs> {
        &self.session
    }

    // Execute the mount operation. Requires a mutable reference to self to manage the session state.
    #[instrument(skip(self, fs), err(level = Level::ERROR))]
    pub async fn mount(&mut self, fs: Fs) -> Result<()> {
        tracing::info!("Mounting FS at {:?}", self.mountpoint);

        self.session
            .mount(fs, &self.mountpoint, &self.options)
            .await?;

        tracing::info!("FS mounted at {:?}", self.mountpoint);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn wait(&mut self) -> Result<()> {
        tracing::info!("Waiting for FS unmount or session end...");
        self.session.wait().await?;

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn unmount(&mut self) -> Result<()> {
        tracing::info!("Unmounting FS from {:?}", self.mountpoint);
        self.session.unmount().await?;

        Ok(())
    }
}

impl MountOptions {
    /// Create default MountOptions
    pub fn new() -> Self {
        MountOptionsBuilder::new().build()
    }

    /// Create a new MountOptions builder
    pub fn builder() -> MountOptionsBuilder {
        MountOptionsBuilder::new()
    }

    // TODO: add config-driven options
    /// Create MountOptions from Config using builder
    pub fn from(config: &Config) -> Self {
        MountOptionsBuilder::new()
            // Add config-driven options here if needed
            .build()
    }
}

impl Default for MountOptions {
    fn default() -> Self {
        MountOptionsBuilder::new().build()
    }
}

fn create_driver() -> Box<dyn MountFs> {
    #[cfg(unix)]
    return Box::new(unix::UnixSession::default());
    // #[cfg(windows)]
    // return Box::new(windows::WindowsSession::default());

    // #[cfg(not(any(unix, windows)))]
    #[cfg(not(any(unix)))]
    panic!("Platform not supported");
}

/// Builder for MountOptions
#[derive(Default)]
pub struct MountOptionsBuilder {
    read_only: Option<bool>,
    allow_other: Option<bool>,
    unprivileged: Option<bool>,
}

impl MountOptionsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = Some(read_only);
        self
    }
    pub fn allow_other(mut self, allow_other: bool) -> Self {
        self.allow_other = Some(allow_other);
        self
    }
    pub fn unprivileged(mut self, unprivileged: bool) -> Self {
        self.unprivileged = Some(unprivileged);
        self
    }

    pub fn build(self) -> MountOptions {
        MountOptions {
            read_only: self.read_only.unwrap_or_default(),
            allow_other: self.allow_other.unwrap_or_default(),
            unprivileged: self.unprivileged.unwrap_or_default(),
        }
    }
}
