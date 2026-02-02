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

    async fn lazy_unmount(&mut self, mount_point: &Path) -> Result<()> {
        // Clone the mount_point for use inside blocking task
        let mp = mount_point.to_path_buf();

        let res = tokio::task::spawn_blocking(move || {
            use std::process::Command;

            let candidates: &[(&str, &[&str])] = &[
                ("umount", &["-l"]),
                ("fusermount3", &["-u", "-z"]),
                ("fusermount", &["-u", "-z"]),
            ];

            for (cmd, args) in candidates {
                let mut command = Command::new(cmd);
                for a in *args {
                    command.arg(a);
                }
                command.arg(&mp);
                if command.spawn().is_ok() {
                    return Ok(());
                }
            }

            Err(MountError::UnmountFailed(
                "No suitable lazy-unmount command available".to_string(),
            ))
        })
        .await;

        match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(join_err) => Err(MountError::Other(anyhow::format_err!(
                "Failed to spawn lazy-unmount task: {}",
                join_err
            ))),
        }
    }
}

impl From<&MountOptions> for MountOptionsFuse {
    fn from(options: &MountOptions) -> Self {
        let mut mount_options = MountOptionsFuse::default();
        if options.allow_other {
            mount_options.allow_other(true);
        }
        if options.read_only {
            mount_options.read_only(true);
        }
        mount_options
    }
}
