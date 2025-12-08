use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use tokio;
use std;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use tracing::{Level, instrument};

use crate::error::FsModelError;
use crate::fs_model::attributes::SetAttr;
use crate::network::client::RemoteClient;
use crate::network::models::{ItemType, SerializableFSItem};

pub mod attributes;
pub mod directory;
pub mod file;
pub mod sym_link;

pub use attributes::*;
use crate::cache::*;
pub use directory::*;
pub use file::*;
use crate::fs_model::sym_link::SymLink;
use crate::fuse::Fs;
use crate::rw_buffer::{ReadBuffer, WriteBuffer};

type Result<T> = std::result::Result<T, FsModelError>;

static CURRENT_FH: AtomicU64 = AtomicU64::new(1);
const BUFFER_CAPAICTY: usize = 2*1024*1024;

#[derive(Debug)]
pub struct FileSystem {
    remote_client: RemoteClient,
    file_handlers: std::sync::RwLock<HashMap<u64, PathBuf>>,
    cache: Option<Cache>,
    read_buffer: tokio::sync::RwLock<ReadBuffer>,
    write_buffer: tokio::sync::RwLock<WriteBuffer>,
}

fn get_parent_path(path: &Path) -> PathBuf {
    if path == Path::new("/") {
        return PathBuf::from("/");
    }
    if path.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() || parent == Path::new(".") => {
            PathBuf::from("/")
        }
        Some(parent) => parent.to_path_buf(),
        None => PathBuf::from("/"),
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
    pub fn new(base_url: &str, cache_config: CacheConfig) -> Self {
        Self {
            remote_client: RemoteClient::new(base_url),
            file_handlers: std::sync::RwLock::new(HashMap::new()),
            cache: Cache::from_config(&cache_config),
            read_buffer: tokio::sync::RwLock::new(ReadBuffer::new(BUFFER_CAPAICTY)),
            write_buffer: tokio::sync::RwLock::new(WriteBuffer::new(BUFFER_CAPAICTY)),
        }
    }


    fn cache_get(&self, path: &Path) -> Option<CacheItem> {
        self.cache.as_ref()?.get(path)
    }

    fn cache_put(&self, path: &Path, item: CacheItem, invalidate_attributes: bool) {
        if let Some(cache) = &self.cache {
            cache.put(path.to_path_buf(), item, invalidate_attributes)
        }
    }

    fn cache_put_new(&self, path: &Path, item: CacheItem) {
        if let Some(cache) = &self.cache {
            cache.put_new(path.to_path_buf(), item)
        }
    }

    fn cache_remove(&self, path: &Path) -> Option<CacheItem>{
        if let Some(cache) = &self.cache {
            cache.remove(path)
        }
        else {
            None
        }
    }

    fn cache_write_file(&self, path: &Path, offset: usize, data: &[u8], invalidate_attributes: bool){
        if let Some(name) = path.file_name() {
            let mut file = File::new(name.to_os_string(), None);
            file.write_content(offset, &data);
            self.cache_put(path, CacheItem::File(file), invalidate_attributes);
        }
    }

    fn cache_put_attr(&self, path: &Path, attributes: FileAttr){
        if let Some(name) = path.file_name() {
            let item = match attributes.kind{
                FileType::Directory=> CacheItem::Directory(Directory::new(name.to_os_string(), Some(attributes), None)),
                FileType::RegularFile=> CacheItem::File(File::new(name.to_os_string(), Some(attributes))),
                FileType::Symlink => CacheItem::SymLink(SymLink::new(name.to_os_string(), Some(attributes), None)),
                _ => CacheItem::File(File::new(name.to_os_string(), Some(attributes))),
            };
            self.cache_put(path, item, false);
        }
    }

    fn cache_get_ttl(&self) -> Duration {
        if let Some(cache) = &self.cache {
            cache.ttl
        }
        else {
            Duration::from_secs(0)
        }
    }

    pub fn get_ttl(&self) -> Duration {
        self.cache_get_ttl()
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn get_path_from_fh(&self, fh: u64) -> Result<Option<PathBuf>> {
        let map = self.file_handlers
            .read()
            .map_err(|_| {
                return FsModelError::ConversionFailed;}
            )?;
        Ok(map.get(&fh).cloned())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn readdir(&self, path: &Path) -> Result<Vec<SerializableFSItem>> {
        if let Some(CacheItem::Directory(dir)) = self.cache_get(path) {
            let mut all_children = Vec::new();
            let mut cache_hit = true;

            if let Some(children) = &dir.children {
                for child in children {
                    let child_path = path.join(child);
                    if let Some(item) = self.cache_get(&child_path) {
                        let Ok(serializable) = SerializableFSItem::try_from(&item) else {
                            cache_hit = false;
                            break;
                        };
                        all_children.push(serializable);
                    } else {
                        cache_hit = false;
                        break;
                    }
                }
            }
            else {
                cache_hit = false;
            }

            // cache hit
            if cache_hit {
                let mut result = Vec::new();

                result.push(SerializableFSItem {
                    name: ".".into(),
                    item_type: ItemType::Directory,
                    attributes: self.get_attributes(path).await?,
                });

                let parent = get_parent_path(path);
                result.push(SerializableFSItem {
                    name: "..".into(),
                    item_type: ItemType::Directory,
                    attributes: self.get_attributes(&parent).await?,
                });

                for c in all_children {
                    result.push(c);
                }

                return Ok(result);
            }
        }

        // cache miss
        let elements = self.list_path(path).await?;
        let children_names = elements.iter().map(|e| e.name.clone().into()).collect();

        let name = if let Some(n) = path.file_name() {
            n.to_os_string()
        }
        else {
            OsString::new()
        };

        let dir = Directory::new(name, Some(self.get_attributes(path).await?), Some(children_names));
        self.cache_put(path, CacheItem::Directory(dir), false);

        for element in &elements {
            let child_path = path.join(&element.name);
            self.cache_put(&child_path, CacheItem::from(element.clone()), false);
        }
        let mut result = Vec::new();
        result.push(SerializableFSItem {
            name: ".".into(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(path).await?,
        });
        let parent = get_parent_path(path);
        result.push(SerializableFSItem {
            name: "..".into(),
            item_type: ItemType::Directory,
            attributes: self.get_attributes(&parent).await?,
        });
        result.extend(elements);
        Ok(result)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn list_path(&self, path: &Path) -> Result<Vec<SerializableFSItem>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .list_path(path_str)
            .await
            .map_err(|op| FsModelError::Backend(op))
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


        let attr = self.remote_client
            .write_file(path_str, offset, data)
            .await
            .map_err(|op| FsModelError::Backend(op))?;

        if let Some(name) = path.file_name() {
            let item = CacheItem::File(File::new(name.to_os_string(), Some(attr)));
            self.cache_put_new(path, item);
        }
        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn open(&self, uid: u32, gid: u32, path: &Path, flags: &Flags) -> Result<u64> {
        // TODO: check access

        let fh = CURRENT_FH.fetch_add(1, Ordering::Relaxed);

        let mut guad = self
            .file_handlers
            .write()
            .map_err(|_| FsModelError::FileHandlerError)?;

        guad.insert(fh, path.to_path_buf());

        Ok(fh)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn release(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        flags: &Flags,
        fh: u64,
    ) -> Result<()> {
        // TODO: check access

        let mut guad = self
            .file_handlers
            .write()
            .map_err(|_| FsModelError::FileHandlerError)?;

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
        if let Some(CacheItem::File(file)) = self.cache_get(path){
            let data = file.read(offset, size);
            if data.len() > 0 {
                // cache hit
                return Ok(data);
            }
        }

        {
            let buffer = self.read_buffer.read().await;
            let data = buffer.read(path, offset, size);
            if !data.is_empty() {
                // buffer hit
                self.cache_write_file(path, offset, &data, false);
                return Ok(data);
            }
        }

        // cache & buffer miss
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let data = self.remote_client.read_file(path_str, offset, BUFFER_CAPAICTY).await.map_err(|op| FsModelError::Backend(op))?;
        // Fill buffer
        {
            let mut buffer = self.read_buffer.write().await;
            buffer.fill(path, offset, &data);
        }
        self.cache_write_file(path, offset, &data, false);
        let end = data.len().min(size);
        Ok(data[..end].to_vec())
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

        let mut data_written = 0;

        {
            let mut buffer = self.write_buffer.write().await;
            if !buffer.is_appending(path, offset){
                let (path, buffer_offset, buffer_data) = buffer.get_content();

                let path_str = path
                    .to_str()
                    .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

                if buffer_data.len() > 0 {
                    self.remote_client
                        .write_file(path_str, buffer_offset, buffer_data)
                        .await?;
                }
                buffer.clean();
            }
            data_written = buffer.write(path, offset, data);
            if buffer.is_full(){
                let path_str = path
                    .to_str()
                    .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

                let (_, buffer_offset, buffer_data) = buffer.get_content();

                self.remote_client
                    .write_file(path_str, buffer_offset, buffer_data)
                    .await?;

                buffer.clean();
            }
        }
        self.cache_write_file(path, offset, &data, true);
        Ok(data_written)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &Path) -> Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attr = self.remote_client
            .mkdir(path_str)
            .await
            .map_err(|op| FsModelError::Backend(op))?;

        if let Some(name) = path.file_name() {
            let item = CacheItem::Directory(Directory::new(name.to_os_string(), Some(attr), Some(vec![])));
            self.cache_put_new(path, item);
        }

        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        let old_path_str = old_path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let new_path_str = new_path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;


        self.remote_client
            .rename(old_path_str, new_path_str)
            .await
            .map_err(|op| FsModelError::Backend(op))?;

        let old_item = self.cache_remove(old_path);
        if let Some(name) = new_path.file_name() {
            if let Some(mut item) = old_item {
                item.rename(name.to_os_string());
                self.cache_put_new(new_path, item);
            }
        }

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client.remove(path_str).await.map_err(|op| FsModelError::Backend(op))?;

        self.cache_remove(path);

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
    ) -> anyhow::Result<FileAttr> {
        if let Some(item) = self.cache_get(path) {
            if let Some(attr) = item.get_attributes(){
                // cache hit
                return Ok(attr);
            }
        }

        // cache miss
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self.remote_client.resolve_child(uid, gid, path_str).await.map_err(|op| FsModelError::Backend(op))?;

        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &Path) -> anyhow::Result<FileAttr> {
        if let Some(item) = self.cache_get(path) {
            if let Some(attr) = item.get_attributes(){
                // cache hit
                return Ok(attr);
            }
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self.remote_client.get_attributes(path_str).await.map_err(|op| FsModelError::Backend(op))?;

        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes(
        &self,
        uid: u32,
        gid: u32,
        path: &Path,
        new_attributes: SetAttr,
    ) -> anyhow::Result<FileAttr> {

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self
            .remote_client
            .set_attributes(uid, gid, path_str, new_attributes)
            .await.map_err(|op| FsModelError::Backend(op))?;


        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions(&self, path: &Path) -> anyhow::Result<u32> {
        // TODO: cache

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let permissions = self.remote_client.get_permissions(path_str).await.map_err(|op| FsModelError::Backend(op))?;
        Ok(permissions)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_fs_stats(&self, path: &Path) -> anyhow::Result<Stats> {
        // TODO: cache, can't do it without knowing file type

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let stats = self.remote_client.get_stats(path_str).await.map_err(|op| FsModelError::Backend(op))?;
        Ok(stats)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_symlink(
        &self,
        path: &Path,
        target: &str,
    ) -> Result<FileAttr> {
        // TODO: check access

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;


        let attributes = self.remote_client.create_symlink(path_str, target).await.map_err(|op| FsModelError::Backend(op))?;

        if let Some(name) = path.file_name() {
            let item = CacheItem::SymLink(SymLink::new(name.to_os_string(), Some(attributes), Some(target.to_string())));
            self.cache_put_new(path, item);
        }
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_symlink(
        &self,
        path: &Path,
    ) -> Result<String> {
        // TODO: check access
        if let Some(CacheItem::SymLink(SymLink{target, ..})) = self.cache_get(path){
            if let Some(target) = target{
                // cache hit
                return Ok(target);
            }
        }

        // cache miss
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let target = self.remote_client.read_symlink(path_str).await.map_err(|op| FsModelError::Backend(op))?;
        if let Some(name) = path.file_name() {
            let item = CacheItem::SymLink(SymLink::new(name.to_os_string(), None, Some(target.clone())));
            self.cache_put(path, item, false);
        }

        Ok(target)
    }

    pub async fn flush_write_buffer(&self) -> Result<()> {
        let mut buffer = self.write_buffer.write().await;

        let (path, buffer_offset, buffer_data) = buffer.get_content();

        let path_str = path
                .to_str()
                .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        if buffer_data.len() > 0 {
            self.remote_client
                .write_file(path_str, buffer_offset, buffer_data)
                .await?;
        }
        buffer.clean();
        Ok(())
    }

    pub fn cache_invalidate(&self, path: &Path){
        if let Some(cache) = &self.cache {
            cache.invalidate(path);
        }
    }
}
