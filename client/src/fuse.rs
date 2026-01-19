#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use crate::config::RfsConfig;
use crate::fs_model::FileSystem;
use crate::network::RemoteClient;

pub struct Fs {
    fs: FileSystem,
}

impl Fs {
    pub fn new(rc: RemoteClient, config: &RfsConfig) -> Self {
        Self {
            fs: FileSystem::new(rc, config),
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
