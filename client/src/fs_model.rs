use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::{OsStr, OsString};
use std::fmt::Debug;
use std::path::{self, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{Level, instrument};

use crate::cache::*;
use crate::error::FsModelError;
use crate::network::RemoteClient;
use crate::network::models::{ItemType, SerializableFSItem, Xattributes};
use crate::rw_buffer::{ReadBuffer, WriteBuffer};

pub mod attributes;
pub mod directory;
pub mod file;
pub mod sym_link;

pub use attributes::*;
pub use directory::*;
pub use file::*;
pub use sym_link::*;

type Result<T> = std::result::Result<T, FsModelError>;

static CURRENT_FH: AtomicU64 = AtomicU64::new(1);
const BUFFER_CAPACITY: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct FileSystem {
    remote_client: RemoteClient,
    xattributes_enabled: bool,
    file_handlers: RwLock<HashMap<u64, PathBuf>>,
    read_buffer: RwLock<ReadBuffer>,
    write_buffer: RwLock<WriteBuffer>,
    cache: Option<Cache>,
}

fn get_parent_path<P: AsRef<Path> + Debug>(path: P) -> PathBuf {
    let path = path.as_ref();

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
    // #[instrument(ret(level = Level::DEBUG))]
    pub fn new(rc: RemoteClient, cache_config: CacheConfig, xattributes_enabled: bool) -> Self {
        Self {
            remote_client: rc,
            file_handlers: RwLock::new(HashMap::new()),
            xattributes_enabled,
            read_buffer: RwLock::new(ReadBuffer::new(BUFFER_CAPACITY)),
            write_buffer: RwLock::new(WriteBuffer::new(BUFFER_CAPACITY)),
            cache: Cache::from_config(&cache_config),
        }
    }

    fn cache_get<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        self.cache.as_ref()?.get(path)
    }

    fn cache_put<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        item: CacheItem,
        invalidate_attributes: bool,
    ) {
        if let Some(cache) = &self.cache {
            cache.put(path.as_ref().to_path_buf(), item, invalidate_attributes)
        }
    }

    fn cache_put_new<P: AsRef<Path> + Debug>(&self, path: P, item: CacheItem) {
        if let Some(cache) = &self.cache {
            cache.put_new(path.as_ref().to_path_buf(), item)
        }
    }

    fn cache_remove<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        if let Some(cache) = &self.cache {
            cache.remove(path)
        } else {
            None
        }
    }

    fn cache_write_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        offset: usize,
        data: &[u8],
        invalidate_attributes: bool,
    ) {
        if let Some(name) = path.as_ref().file_name() {
            let mut file = File::new(name.to_os_string(), None);
            file.write_content(offset, &data);
            self.cache_put(path, CacheItem::File(file), invalidate_attributes);
        }
    }

    fn cache_put_attr<P: AsRef<Path> + Debug>(&self, path: P, attributes: Attributes) {
        if let Some(name) = path.as_ref().file_name() {
            let item = match attributes.kind {
                FileType::Directory => CacheItem::Directory(Directory::new(
                    name.to_os_string(),
                    Some(attributes),
                    None,
                )),
                FileType::RegularFile => {
                    CacheItem::File(File::new(name.to_os_string(), Some(attributes)))
                }
                FileType::Symlink => {
                    CacheItem::SymLink(SymLink::new(name.to_os_string(), Some(attributes), None))
                }
                _ => CacheItem::File(File::new(name.to_os_string(), Some(attributes))),
            };
            self.cache_put(path, item, false);
        }
    }

    fn cache_get_ttl(&self) -> Duration {
        if let Some(cache) = &self.cache {
            cache.ttl
        } else {
            Duration::from_secs(0)
        }
    }

    pub fn get_ttl(&self) -> Duration {
        self.cache_get_ttl()
    }

    pub fn use_xattributes(&self) -> bool {
        self.xattributes_enabled
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_path_from_fh(&self, fh: u64) -> Result<Option<PathBuf>> {
        let map = self.file_handlers.read().await;
        Ok(map.get(&fh).cloned())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn readdir<P: AsRef<Path> + Debug>(
        &self,
        path: P,
    ) -> Result<Vec<SerializableFSItem>> {
        let path = path.as_ref();

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
            } else {
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
        } else {
            OsString::new()
        };

        let dir = Directory::new(
            name,
            Some(self.get_attributes(path).await?),
            Some(children_names),
        );
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
    async fn list_path<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Vec<SerializableFSItem>> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let list_items = self.remote_client.list_path(path_str).await?;
        Ok(list_items)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        file_type: &FileType,
        offset: usize,
        data: &[u8],
    ) -> Result<Attributes> {
        // TODO: flags

        let path = path.as_ref();
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attr = self
            .remote_client
            .write_file(path_str, offset, data.to_vec())
            .await?;

        if let Some(name) = path.file_name() {
            let item = CacheItem::File(File::new(name.to_os_string(), Some(attr)));
            self.cache_put_new(path, item);
        }
        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn open<P: AsRef<Path> + Debug>(&self, path: P, flags: &Flags) -> Result<u64> {
        // TODO: flags

        let fh = CURRENT_FH.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.file_handlers.write().await;

        guard.insert(fh, path.as_ref().to_path_buf());

        Ok(fh)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn release<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        flags: &Flags,
        fh: u64,
    ) -> Result<()> {
        // TODO: flags

        let mut guard = self.file_handlers.write().await;

        guard.remove(&fh);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let path = path.as_ref();

        if let Some(CacheItem::File(file)) = self.cache_get(path) {
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

        let data = self
            .remote_client
            .read_file(path_str, offset, BUFFER_CAPACITY)
            .await?;

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
    pub async fn write_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        flags: &Flags,
        offset: usize,
        data: &[u8],
    ) -> Result<usize> {
        /*
        // TODO flags
        mi dà errore nelle copy
        if !(flags.writeonly || flags.readwrite) {
            return Err(FsModelError::PermissionDenied(String::from(
                "You do not have enough permissions.",
            )));
        }
         */

        let path = path.as_ref();
        let mut data_written = 0;

        let mut uploads: Vec<(String, usize, Vec<u8>)> = Vec::new();

        {
            let mut buffer = self.write_buffer.write().await;

            if !buffer.is_appending(path, offset) {
                let (buf_path, buf_offset, buf_data) = buffer.get_content();
                if !buf_data.is_empty() {
                    uploads.push((
                        buf_path.to_string_lossy().to_string(),
                        buf_offset,
                        buf_data.to_vec(),
                    ));
                }
                buffer.clean();
            }

            data_written = buffer.write(path, offset, data);

            if buffer.is_full() {
                let (buf_path, buf_offset, buf_data) = buffer.get_content();
                uploads.push((
                    buf_path.to_string_lossy().to_string(),
                    buf_offset,
                    buf_data.to_vec(),
                ));
                buffer.clean();
            }
        }

        for (path, offset, data) in uploads {
            let client = self.remote_client.clone();
            tokio::spawn(async move {
                let _ = client.write_file(&path, offset, data).await;
            });
        }

        self.cache_write_file(path, offset, data, true);
        Ok(data_written)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Attributes> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attr = self.remote_client.mkdir(path_str).await?;

        if let Some(name) = path.as_ref().file_name() {
            let item = CacheItem::Directory(Directory::new(
                name.to_os_string(),
                Some(attr),
                Some(vec![]),
            ));
            self.cache_put_new(path, item);
        }

        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename<P: AsRef<Path> + Debug>(&self, old_path: P, new_path: P) -> Result<()> {
        let old_path_str = old_path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let new_path_str = new_path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .rename(old_path_str, new_path_str)
            .await?;

        let old_item = self.cache_remove(old_path);
        if let Some(name) = new_path.as_ref().file_name() {
            if let Some(mut item) = old_item {
                item.rename(name.to_os_string());
                self.cache_put_new(new_path, item);
            }
        }

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove<P: AsRef<Path> + Debug>(&self, path: P) -> Result<()> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client.remove(path_str).await?;
        self.cache_remove(path);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Attributes> {
        let path = path.as_ref();

        if let Some(item) = self.cache_get(path) {
            if let Some(attr) = item.get_attributes() {
                // cache hit
                return Ok(attr);
            }
        }

        // cache miss
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self.remote_client.resolve_child(path_str).await?;

        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Attributes> {
        let path = path.as_ref();

        if let Some(item) = self.cache_get(path) {
            if let Some(attr) = item.get_attributes() {
                // cache hit
                return Ok(attr);
            }
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self.remote_client.get_attributes(path_str).await?;

        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes<P: AsRef<Path> + Debug>(
        &self,
        uid: u32,
        gid: u32,
        path: P,
        new_attributes: SetAttr,
    ) -> Result<Attributes> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self
            .remote_client
            .set_attributes(uid, gid, path_str, new_attributes)
            .await?;

        self.cache_put_attr(path, attributes);
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions<P: AsRef<Path> + Debug>(&self, path: P) -> Result<u32> {
        // TODO: cache

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let permissions = self.remote_client.get_permissions(path_str).await?;
        Ok(permissions)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_fs_stats<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Stats> {
        // No cache here

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let stats = self.remote_client.get_stats(path_str).await?;
        Ok(stats)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
    ) -> Result<Vec<u8>> {
        // No cache here

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let xattributes = self.remote_client.get_x_attributes(path_str, name).await?;
        Ok(xattributes.get())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
        xattributes: &[u8],
        flags: u32,
        position: u32,
    ) -> Result<()> {
        // No cache here

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .set_x_attributes(path_str, name, xattributes)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attribute<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Vec<String>> {
        // No cache here

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let names = self.remote_client.list_x_attributes(path_str).await?;
        Ok(names)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
    ) -> Result<()> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        self.remote_client
            .remove_x_attributes(path_str, name)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_symlink<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        target: &str,
    ) -> Result<Attributes> {
        // TODO: check access

        let path = path.as_ref();
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let attributes = self.remote_client.create_symlink(path_str, target).await?;

        if let Some(name) = path.file_name() {
            let item = CacheItem::SymLink(SymLink::new(
                name.to_os_string(),
                Some(attributes),
                Some(target.to_string()),
            ));
            self.cache_put_new(path, item);
        }
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_symlink<P: AsRef<Path> + Debug>(&self, path: P) -> Result<String> {
        // TODO: check access
        if let Some(CacheItem::SymLink(SymLink { target, .. })) = self.cache_get(path.as_ref()) {
            if let Some(target) = target {
                // cache hit
                return Ok(target);
            }
        }

        // cache miss
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))?;

        let target = self.remote_client.read_symlink(path_str).await?;

        if let Some(name) = path.as_ref().file_name() {
            let item = CacheItem::SymLink(SymLink::new(
                name.to_os_string(),
                None,
                Some(target.clone()),
            ));
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
        let path_string = path_str.to_string();
        let buffer_data = buffer_data.to_vec();
        if buffer_data.len() > 0 {
            let client = self.remote_client.clone();
            tokio::spawn(async move {
                let _ = client
                    .write_file(&path_string, buffer_offset, buffer_data)
                    .await;
            });
        }
        buffer.clean();
        Ok(())
    }

    pub fn cache_invalidate<P: AsRef<Path> + Debug>(&self, path: P) {
        if let Some(cache) = &self.cache {
            cache.invalidate(path);
        }
    }
}
