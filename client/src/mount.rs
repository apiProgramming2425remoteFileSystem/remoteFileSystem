#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod windows;

use std::path::{Path, PathBuf};

use crate::config::mount::MountConfig;
use crate::error::MountError;
use crate::fuse::Fs;

use async_trait::async_trait;
use tracing::{Level, instrument};

type Result<T> = std::result::Result<T, MountError>;

/// Mount point representation
pub struct MountPoint {
    mount_point: PathBuf,
    options: MountOptions,
    session: Box<dyn MountFs>,
}

/// Mount configuration
#[derive(Debug)]
pub struct MountOptions {
    read_only: bool,
    allow_other: bool,
    privileged: bool,
}

/// Trait for mounting and unmounting the filesystem.
#[async_trait]
pub trait MountFs: Send + Sync {
    /// Mounts the filesystem at the specified mount point.
    #[allow(unused_variables)]
    async fn mount(&mut self, fs: Fs, mount_point: &Path, options: &MountOptions) -> Result<()> {
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

    /// Unmounts the filesystem from the mount point.
    async fn unmount(&mut self) -> Result<()> {
        Err(MountError::UnsupportedPlatform(
            "Unmounting not supported on this platform".into(),
        ))
    }

    /// Attempt a lazy/detach unmount for the given mount point.
    async fn lazy_unmount(&mut self, _mount_point: &Path) -> Result<()> {
        Err(MountError::UnsupportedPlatform(
            "Lazy unmount not supported on this platform".into(),
        ))
    }
}

impl MountPoint {
    pub fn new<P: AsRef<Path>>(mount_point: P, options: MountOptions) -> Self {
        let driver = create_driver();

        Self {
            mount_point: mount_point.as_ref().to_path_buf(),
            options,
            session: driver,
        }
    }

    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }
    pub fn options(&self) -> &MountOptions {
        &self.options
    }
    pub fn session(&self) -> &dyn MountFs {
        &*self.session
    }

    // Execute the mount operation. Requires a mutable reference to self to manage the session state.
    #[instrument(skip(self, fs), err(level = Level::ERROR))]
    pub async fn mount(&mut self, fs: Fs) -> Result<()> {
        tracing::info!("Mounting FS at {:?}", self.mount_point);

        self.session
            .mount(fs, &self.mount_point, &self.options)
            .await?;

        tracing::info!("FS mounted at {:?}", self.mount_point);
        Ok(())
    }

    // Delegate to the platform-specific `wait` implementation.
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn wait(&mut self) -> Result<()> {
        tracing::info!("Waiting for FS unmount or session end...");
        self.session.wait().await?;

        Ok(())
    }

    // Delegate to the platform-specific `unmount` implementation.
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn unmount(&mut self) -> Result<()> {
        tracing::info!("Unmounting FS from {:?}", self.mount_point);
        self.session.unmount().await?;

        Ok(())
    }

    /// Delegate to the platform-specific `lazy_unmount` implementation.
    #[instrument(skip(self), err(level = Level::WARN))]
    pub async fn lazy_unmount(&mut self) -> Result<()> {
        self.session.lazy_unmount(&self.mount_point).await
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

    /// Create MountOptions from Config using builder
    pub fn from(config: &MountConfig) -> Self {
        MountOptionsBuilder::new()
            .read_only(config.read_only)
            .allow_other(config.allow_other)
            .privileged(config.privileged)
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
    privileged: Option<bool>,
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
    pub fn privileged(mut self, privileged: bool) -> Self {
        self.privileged = Some(privileged);
        self
    }

    pub fn build(self) -> MountOptions {
        MountOptions {
            read_only: self.read_only.unwrap_or_default(),
            allow_other: self.allow_other.unwrap_or_default(),
            privileged: self.privileged.unwrap_or_default(),
        }
    }
}
