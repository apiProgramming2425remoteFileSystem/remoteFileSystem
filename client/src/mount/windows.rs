use super::*;
use async_trait::async_trait;
use winfsp::host::{FileSystemHost, VolumeParams};

#[derive(Default)]
pub struct WindowsSession {
    host: Option<FileSystemHost<Fs>>,
}

#[async_trait]
impl MountFs for WindowsSession {
    async fn mount(&mut self, fs: Fs, mount_point: &Path, _options: &MountOptions) -> Result<()> {
        let mut params = VolumeParams::default();
        params.case_preserved_names(true);

        let mut host =
            FileSystemHost::new(params, fs).map_err(|e| MountError::MountFailed(e.to_string()))?;

        host.start()
            .map_err(|e| MountError::MountFailed(e.to_string()))?;

        host.mount(mount_point)
            .map_err(|e| MountError::MountFailed(e.to_string()))?;

        self.host = Some(host);

        Ok(())
    }

    async fn wait(&mut self) -> Result<()> {
        tokio::task::spawn_blocking(|| {
            std::thread::park();
        })
        .await
        .map_err(|e| MountError::Other(e.into()))?;

        Ok(())
    }

    async fn unmount(&mut self) -> Result<()> {
        if let Some(mut host) = self.host.take() {
            host.unmount();
        }

        Ok(())
    }
}
