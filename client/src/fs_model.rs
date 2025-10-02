use std::ffi::OsStr;
use std::path::Path;
use std::time::SystemTime;

use tracing::{Level, instrument};

use crate::network::client::RemoteClient;
use crate::network::models::SerializableFSItem;

pub mod attributes;
pub mod directory;
pub mod file;

pub use attributes::*;
// pub use directory::*;
// pub use file::*;

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
    pub async fn list_path(&self, path: &OsStr) -> anyhow::Result<Vec<SerializableFSItem>> {
        self.remote_client.list_path(path).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_file(
        &self,
        uid: u32,
        gid: u32,
        parent: &Path,
        name: &Path,
        file_type: &FileType,
    ) -> anyhow::Result<FileAttr> {
        // TODO: check access

        let path = parent.join(name);
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        self.remote_client
            .write_file(path_str, 0, &Vec::new())
            .await?;

        Ok(self.mock_file_attr())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn open_file(&self, uid: u32, gid: u32, path: &Path, flags: &Flags) -> anyhow::Result<u64> {
        // TODO: check access

        // TODO: assign file_handle
        Ok(0)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        offset: usize,
        size: usize,
    ) -> anyhow::Result<Vec<u8>> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let data = self.remote_client.read_file(path_str, offset, size).await?;
        Ok(data)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn write_file(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        flags: &Flags,
        offset: usize,
        data: &[u8],
    ) -> anyhow::Result<usize> {
        // TODO: check access

        if !(flags.writeonly || flags.readwrite) {
            return Err(anyhow::anyhow!("No write access"));
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        self.remote_client
            .write_file(path_str, offset, data)
            .await?;

        Ok(data.len())
    }

    pub async fn mkdir(&self, path: &OsStr) -> anyhow::Result<()> {
        self.remote_client.mkdir(path).await
    }

    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr) -> anyhow::Result<()> {
        self.remote_client.rename(old_path, new_path).await
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
            crtime: SystemTime::now(),
            kind: FileType::Directory,
            perm: Permission::try_from(0o755 as u16).unwrap(),
            nlink: 2,
            uid: 1,
            gid: 1,
            rdev: 0,
            flags: 0,
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
            crtime: SystemTime::now(),
            kind: FileType::RegularFile,
            perm: Permission::try_from(0o755 as u16).unwrap(),
            nlink: 2,
            uid: 1,
            gid: 1,
            rdev: 0,
            flags: 0,
        }
    }
}
