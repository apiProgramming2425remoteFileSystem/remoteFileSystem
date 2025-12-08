use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use tracing::{Level, instrument};

use crate::error::FsModelError;
use crate::network::RemoteClient;
use crate::network::models::{ItemType, SerializableFSItem};

pub mod attributes;
pub use attributes::*;

type Result<T> = std::result::Result<T, FsModelError>;

static CURRENT_FH: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct FileSystem {
    remote_client: RemoteClient,
    file_handlers: RwLock<HashMap<u64, OsString>>,
}

fn get_parent_path(path: &OsStr) -> OsString {
    let p = Path::new(path);
    if p.as_os_str().is_empty() {
        return OsString::from("/");
    }
    if let Some(par) = p.parent() {
        if par.as_os_str() == "." {
            OsString::from("/")
        } else {
            par.into()
        }
    } else {
        OsString::from("/")
    }
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
            file_handlers: RwLock::new(HashMap::new()),
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn get_path_from_fh(&self, fh: u64) -> Result<Option<OsString>> {
        let map = self
            .file_handlers
            .read()
            .map_err(|_| FsModelError::ConversionFailed)?;
        Ok(map.get(&fh).cloned())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn readdir(&self, path: &OsStr) -> Result<Vec<SerializableFSItem>> {
        let mut items: Vec<SerializableFSItem> = vec![];
        items.push(SerializableFSItem {
            name: ".".to_string(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(path).await?,
        });
        let parent_path = get_parent_path(path);
        items.push(SerializableFSItem {
            name: "..".to_string(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(&parent_path).await?,
        });
        items.extend(self.list_path(path).await?);
        Ok(items)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn list_path(&self, path: &OsStr) -> Result<Vec<SerializableFSItem>> {
        let file_list = self.remote_client.list_path(path).await?;
        Ok(file_list)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_file(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        file_type: &FileType,
        offset: usize,
        data: &[u8],
    ) -> Result<FileAttr> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .write_file(path_str, offset, data)
            .await?;

        Ok(self.mock_file_attr())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn open(&self, uid: u32, gid: u32, path: &OsStr, flags: &Flags) -> Result<u64> {
        // TODO: check access

        let fh = CURRENT_FH.fetch_add(1, Ordering::Relaxed);

        let mut guad = self
            .file_handlers
            .write()
            .map_err(|_| anyhow::anyhow!(""))?;

        guad.insert(fh, path.to_os_string());

        Ok(fh)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn release(&self, uid: u32, gid: u32, path: &Path, flags: &Flags, fh: u64) -> Result<()> {
        // TODO: check access

        let mut guad = self
            .file_handlers
            .write()
            .map_err(|_| anyhow::anyhow!(""))?;

        guad.remove(&fh);

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

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
    ) -> Result<usize> {
        // TODO: check access

        if !(flags.writeonly || flags.readwrite) {
            return Err(FsModelError::PermissionDenied);
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .write_file(path_str, offset, data)
            .await?;

        Ok(data.len())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &OsStr) -> Result<FileAttr> {
        let file_attr = self.remote_client.mkdir(path).await?;
        Ok(file_attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr) -> Result<()> {
        self.remote_client.rename(old_path, new_path).await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &OsStr) -> Result<()> {
        self.remote_client.remove(path).await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
    ) -> anyhow::Result<FileAttr> {
        let attributes = self.remote_client.resolve_child(uid, gid, path).await?;

        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &OsStr) -> Result<FileAttr> {
        let attributes = self.remote_client.get_attributes(path).await?;
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
        new_attributes: SetAttr,
    ) -> anyhow::Result<FileAttr> {
        let attributes = self
            .remote_client
            .set_attributes(uid, gid, path, new_attributes)
            .await?;
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions(&self, path: &OsStr) -> anyhow::Result<u32> {
        let permissions = self.remote_client.get_permissions(path).await?;
        Ok(permissions)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_fs_stats(&self, path: &OsStr) -> anyhow::Result<Stats> {
        let stats = self.remote_client.get_stats(path).await?;
        Ok(stats)
    }

    // TODO: remove it later
    pub fn mock_dir_attr(&self) -> FileAttr {
        FileAttr {
            size: 0,
            blocks: 0,
            blksize: 0,
            atime: Timestamp::from(SystemTime::now()),
            mtime: Timestamp::from(SystemTime::now()),
            ctime: Timestamp::from(SystemTime::now()),
            crtime: Timestamp::from(SystemTime::now()),
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
            atime: Timestamp::from(SystemTime::now()),
            mtime: Timestamp::from(SystemTime::now()),
            ctime: Timestamp::from(SystemTime::now()),
            crtime: Timestamp::from(SystemTime::now()),
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
