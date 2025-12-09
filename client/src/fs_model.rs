use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use tracing::{Level, instrument};

use crate::error::FsModelError;
use crate::fs_model::attributes::SetAttr;
use crate::network::client::RemoteClient;
use crate::network::models::{ItemType, SerializableFSItem, Xattributes};

pub mod attributes;
pub mod directory;
pub mod file;

pub use attributes::*;
// pub use directory::*;
// pub use file::*;

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
    pub fn new(rc: RemoteClient) -> Self {
        Self {
            remote_client: rc,
            file_handlers: RwLock::new(HashMap::new()),
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn get_path_from_fh(&self, fh: u64) -> Result<Option<OsString>> {
        let map = self.file_handlers.read().map_err(|_| { return FsModelError::ConversionFailed(String::new());})?;
        Ok(map.get(&fh).cloned())
    }



    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn readdir(&self, path: &OsStr, token: &str) -> Result<Vec<SerializableFSItem>> {
        let mut items: Vec<SerializableFSItem> = vec![];
        items.push(SerializableFSItem{
            name: ".".to_string(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(path, token).await?,
        });
        let parent_path = get_parent_path(path);
        items.push(SerializableFSItem{
            name: "..".to_string(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(&parent_path, token).await?,
        });
        items.extend(self.list_path(path, token).await?);
        Ok(items)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn list_path(&self, path: &OsStr, token: &str) -> Result<Vec<SerializableFSItem>> {
        self.remote_client
            .list_path(path, token)
            .await
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
        token: &str,
    ) -> Result<FileAttr> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .write_file(path_str, offset, data, token)
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
    pub fn release(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        flags: &Flags,
        fh: u64
    ) -> Result<()> {
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
        token: &str
    ) -> Result<Vec<u8>> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let data = self.remote_client.read_file(path_str, offset, size, token).await?;
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
        token: &str
    ) -> Result<usize> {
        // TODO: check access

        if !(flags.writeonly || flags.readwrite) {
            return Err(FsModelError::PermissionDenied(String::from("You do not have enough permissions.")));
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .write_file(path_str, offset, data, token)
            .await?;

        Ok(data.len())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &OsStr, token: &str) -> Result<FileAttr> {
        self.remote_client
            .mkdir(path, token)
            .await
            .map_err(|op| FsModelError::Backend(op))
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr, token: &str) -> Result<()> {
        self.remote_client
            .rename(old_path, new_path, token)
            .await
            .map_err(|op| FsModelError::Backend(op))
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &OsStr, token:&str) -> anyhow::Result<()> {
        self.remote_client.remove(path, token).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
        token: &str
    ) -> anyhow::Result<FileAttr> {
        let attributes = self.remote_client.resolve_child(uid, gid, path, token).await?;

        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &OsStr, token: &str) -> Result<FileAttr> {
        let attributes = self.remote_client.get_attributes(path, token).await?;
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
        new_attributes: SetAttr,
        token: &str
    ) -> anyhow::Result<FileAttr> {
        let attributes = self
            .remote_client
            .set_attributes(uid, gid, path, new_attributes, token)
            .await?;
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions(&self, path: &OsStr, token: &str) -> anyhow::Result<u32> {
        let permissions = self.remote_client.get_permissions(path, token).await?;
        Ok(permissions)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_fs_stats(&self, path: &OsStr, token: &str) -> anyhow::Result<Stats> {
        let stats = self.remote_client.get_stats(path, token).await?;
        Ok(stats)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(&self, path: &OsStr, name: &str, token: &str) -> Result<Xattributes>{
        let xattributes = self.remote_client.get_x_attributes(path, name, token).await?;
        Ok(xattributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_x_attributes(&self, path: &OsStr, name: &str, xattributes: &[u8], token: &str) -> Result<()>{
        self.remote_client.set_x_attributes(path, name, xattributes, token).await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attribute(&self, path: &OsStr, token: &str) -> Result<Vec<String>>{
        let names = self.remote_client.list_x_attributes(path, token).await?;
        Ok(names)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes(&self, path: &OsStr, name: &str, token: &str) -> Result<()> {
        self.remote_client.remove_x_attributes(path, name, token).await?;
        Ok(())
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
