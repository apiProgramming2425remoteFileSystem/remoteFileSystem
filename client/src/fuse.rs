#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::*;

use crate::config::RfsConfig;
use crate::fs_model::FileSystem;
use crate::network::RemoteStorage;
#[cfg(windows)]
use tokio::runtime::Runtime;

pub struct Fs {
    fs: FileSystem,
    #[cfg(windows)]
    rt: Runtime,
}

impl Fs {
    pub fn new<R: RemoteStorage>(rc: R, config: &RfsConfig) -> Self {
        Self {
            fs: FileSystem::new(rc, config),
            #[cfg(windows)]
            rt: Runtime::new().unwrap(),
        }
    }
}

/*
pub async fn template_fn(&self, args) -> Result<> {
    1. convert args to fs_model structures
    2. call the needed self.fs function
    3. converts the result
    4. do other necessary operations
    5. return the correct result
}
*/
