use std::ffi::OsStr;
use std::time::SystemTime;

use fuse3::path::prelude::*;
use tracing::{Level, instrument};

use crate::network::client::RemoteClient;
use crate::network::models::SerializableFSItem;

#[derive(Debug)]
pub struct FileSystem {
    remote_client: RemoteClient,
}

/// pub async fn template_fn(&self, args) -> Result<> {
///     1. check args
///     2. if needed check cache and return result if valid
///     3. do necessary operations (calls the backend)
///     4. save/update result on cache
///     5. return result (the return structure need to be a fs_model structure)
/// }
//
impl FileSystem {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(base_url: &str) -> Self {
        Self {
            remote_client: RemoteClient::new(base_url),
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn fetch_list_path(&self, path: &OsStr) -> anyhow::Result<Vec<SerializableFSItem>> {
        self.remote_client.list_path(path).await
    }

    // TODO: remove it later
    pub fn mock_dir_attr(&self) -> FileAttr {
        FileAttr {
            size: 0,
            blocks: 0,
            blksize: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: 1,
            gid: 1,
            rdev: 0,
        }
    }

    // TODO: remove it
    pub fn mock_file_attr(&self) -> FileAttr {
        FileAttr {
            size: 0,
            blocks: 0,
            blksize: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            kind: FileType::RegularFile,
            perm: 0o755,
            nlink: 2,
            uid: 1,
            gid: 1,
            rdev: 0,
        }
    }
}
