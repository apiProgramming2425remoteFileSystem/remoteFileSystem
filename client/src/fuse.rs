#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::fmt::Debug;

use crate::cache::CacheConfig;
use crate::fs_model::FileSystem;
use crate::network::RemoteStorage;

pub struct Fs {
    fs: FileSystem,
}

impl Fs {
    pub fn new<R: RemoteStorage + Debug + 'static>(
        rc: R,
        cache_config: CacheConfig,
        xattributes_enabled: bool,
    ) -> Self {
        Self {
            fs: FileSystem::new(rc, cache_config, xattributes_enabled),
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
