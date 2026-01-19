use winfsp::host::{FileSystemHost, VolumeParams};
use crate::fuse::Fs;
use crate::cache::CacheConfig;
use crate::network::RemoteClient;

pub fn mount_windows(rc: RemoteClient, cache_config: CacheConfig) -> winfsp::Result<()> {
    let fs = Fs::new(rc, cache_config, false);
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