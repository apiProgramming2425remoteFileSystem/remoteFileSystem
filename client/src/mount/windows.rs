use crate::config::RfsConfig;
use crate::fuse::Fs;
use crate::network::RemoteStorage;
use winfsp::host::{FileSystemHost, VolumeParams};

pub fn mount_windows<R: RemoteStorage>(rc: R, config: &RfsConfig) -> winfsp::Result<()> {
    let fs = Fs::new(rc, config);
    let mut params = VolumeParams::default();
    params.case_preserved_names(true);
    let mut host = FileSystemHost::new(params, fs)?;
    host.start()?;
    println!("[WinFSP] mounting filesystem on X:");
    host.mount("X:")?;
    println!("[WinFSP] mounted successfully");
    println!("[WinFSP] mounted on X:, press Ctrl+C to exit");
    std::thread::park();
    Ok(())
}
