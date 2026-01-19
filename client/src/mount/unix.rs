use super::*;

use anyhow;
use async_trait::async_trait;
use fuse3::MountOptions as MountOptionsFuse;
use fuse3::path::prelude::*;
use fuse3::raw::MountHandle;

#[derive(Default)]
pub struct UnixSession {
    handle: Option<MountHandle>,
}

#[async_trait]
impl MountFs for UnixSession {
    async fn mount(&mut self, fs: Fs, mount_point: &Path, options: &MountOptions) -> Result<()> {
        // Mount fs
        let mount_options = MountOptionsFuse::from(options);
        let session = Session::new(mount_options);

        let mount_handle = if options.privileged {
            tracing::info!("Mounting with privileged user FUSE.");
            session.mount(fs, mount_point).await
        } else {
            tracing::info!("Mounting with unprivileged user FUSE.");
            session.mount_with_unprivileged(fs, mount_point).await
        }
        .map_err(|err| MountError::MountFailed(err.to_string()))?;

        self.handle = Some(mount_handle);
        Ok(())
    }

    async fn wait(&mut self) -> Result<()> {
        let Some(handle) = self.handle.as_mut() else {
            return Err(MountError::Other(anyhow::format_err!(
                "Mount session not initialized."
            )));
        };

        handle.await.map_err(|err| MountError::Other(err.into()))?;

        tracing::info!("Session ended successfully.");
        Ok(())
    }

    async fn unmount(&mut self) -> Result<()> {
        let Some(handle) = self.handle.take() else {
            return Err(MountError::Other(anyhow::format_err!(
                "Mount session not initialized."
            )));
        };

        handle
            .unmount()
            .await
            .map_err(|err| MountError::UnmountFailed(err.to_string()))?;

        tracing::info!("FS unmounted successfully.");
        Ok(())
    }
}

impl From<&MountOptions> for MountOptionsFuse {
    fn from(options: &MountOptions) -> Self {
        let mut mount_options = MountOptionsFuse::default();
        if options.allow_root {
            mount_options.allow_root(true);
        }
        if options.allow_other {
            mount_options.allow_other(true);
        }
        if options.read_only {
            mount_options.read_only(true);
        }
        mount_options
    }
}
