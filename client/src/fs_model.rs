use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::OsString;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{Level, instrument};

use crate::cache::*;
use crate::config::RfsConfig;
use crate::error::FsModelError;
use crate::network::RemoteStorage;
use crate::network::models::{ItemType, SerializableFSItem};
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
pub static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
pub static MAX_PAGES: AtomicUsize = AtomicUsize::new(0);

bitflags::bitflags! {
    #[derive(Debug, Copy, Clone)]
    pub struct RenameFlags: u32 {
        const NOREPLACE = 0b0001;
        const EXCHANGE  = 0b0010;
        const WHITEOUT  = 0b0100;
    }
}

#[derive(Debug)]
pub struct FileSystem {
    remote_client: Arc<dyn RemoteStorage>,
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
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<R: RemoteStorage>(rc: Arc<R>, config: &RfsConfig) -> Self {
        let buffer_capacity = config.file_system.buffer_size;
        let page_size = config.file_system.page_size;

        PAGE_SIZE.store(page_size, Ordering::SeqCst);
        MAX_PAGES.store(config.cache.max_size / page_size, Ordering::SeqCst);

        Self {
            remote_client: rc,
            file_handlers: RwLock::new(HashMap::new()),
            xattributes_enabled: config.file_system.xattr_enable,
            read_buffer: RwLock::new(ReadBuffer::new(buffer_capacity)),
            write_buffer: RwLock::new(WriteBuffer::new(buffer_capacity)),
            cache: Cache::from_config(&config.cache),
        }
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

        if let Some(cache) = &self.cache
            && let Some(CacheItem::Directory(dir)) = cache.get(path).await
        {
            let mut all_children = Vec::new();
            let mut cache_hit = true;

            if let Some(children) = &dir.children {
                for child in children {
                    let child_path = path.join(child);
                    if let Some(item) = cache.get(&child_path).await {
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
        if let Some(cache) = &self.cache {
            cache.put(path, CacheItem::Directory(dir), false).await;
            for element in &elements {
                let child_path = path.join(&element.name);
                cache
                    .put(&child_path, CacheItem::from(element.clone()), false)
                    .await;
            }
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
        let path_str = path_to_string(&path)?;

        let list_items = self.remote_client.list_path(&path_str).await?;
        Ok(list_items)
    }

    #[instrument(skip(self, data), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        file_type: &FileType,
        offset: usize,
        data: &[u8],
    ) -> Result<Attributes> {
        let path = path.as_ref();
        let path_str = path_to_string(path)?;

        let attr = self
            .remote_client
            .write_file(&path_str, offset, data)
            .await?;

        if let (Some(name), Some(cache)) = (path.file_name(), &self.cache) {
            let item = CacheItem::File(File::new(name.to_os_string(), Some(attr)));
            cache.put_new(path, item).await;
        }
        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn open<P: AsRef<Path> + Debug>(&self, path: P, flags: &Flags) -> Result<u64> {
        let fh = CURRENT_FH.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.file_handlers.write().await;

        guard.insert(fh, path.as_ref().to_path_buf());

        Ok(fh)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn release<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        flags: &Flags,
        fh: u64,
    ) -> Result<()> {
        let mut guard = self.file_handlers.write().await;

        guard.remove(&fh);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn read_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let path = path.as_ref();

        if let Ok(attr) = self.get_attributes(path).await
            && offset >= attr.size as usize
        {
            // requested offset is beyond file size, return empty data
            return Ok(vec![]);
        }

        if let Some(cache) = &self.cache
            && let Some(CacheItem::File(file)) = cache.get(path).await
        {
            let data = file.read(offset, size);
            if !data.is_empty() {
                // cache hit
                return Ok(data);
            }
        }

        {
            let buffer = self.read_buffer.read().await;
            let data = buffer.read(path, offset, size);
            if !data.is_empty() {
                // buffer hit
                if let Some(cache) = &self.cache {
                    cache_write_file(cache, path, offset, &data, false).await;
                }
                return Ok(data);
            }
        }

        // cache & buffer miss
        let path_str = path_to_string(path)?;

        let data = self
            .remote_client
            .read_file(&path_str, offset, self.read_buffer.read().await.capacity())
            .await?;

        // Fill buffer
        {
            let mut buffer = self.read_buffer.write().await;
            buffer.fill(path, offset, &data);
        }
        if let Some(cache) = &self.cache {
            cache_write_file(cache, path, offset, &data, false).await;
        }
        let end = data.len().min(size);
        Ok(data[..end].to_vec())
    }

    #[instrument(skip(self, data), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn write_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        flags: &Flags,
        offset: usize,
        data: &[u8],
    ) -> Result<usize> {
        let path = path.as_ref();

        let data_written: usize;
        let mut uploads: Vec<(PathBuf, usize, Vec<u8>)> = Vec::new();

        {
            let mut buffer = self.write_buffer.write().await;

            if !buffer.is_appending(path, offset) {
                let (buf_path, buf_offset, buf_data) = buffer.get_content();
                if !buf_data.is_empty() {
                    uploads.push((buf_path.to_path_buf(), buf_offset, buf_data.to_vec()));
                }
                buffer.clean();
            }

            data_written = buffer.write(path, offset, data);

            if buffer.is_full() {
                let (buf_path, buf_offset, buf_data) = buffer.get_content();
                uploads.push((buf_path.to_path_buf(), buf_offset, buf_data.to_vec()));
                buffer.clean();
            }
        }

        for (path, offset, data) in uploads {
            let path_str = path_to_string(&path)?;
            self.remote_client
                .write_file(&path_str, offset, &data)
                .await?;
            if let Some(cache) = &self.cache {
                cache_write_file(cache, &path, offset, &data, true).await;
            }
        }

        Ok(data_written)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Attributes> {
        let path_str = path_to_string(&path)?;
        let attr = self.remote_client.mkdir(&path_str).await?;

        if let (Some(name), Some(cache)) = (path.as_ref().file_name(), &self.cache) {
            let item = CacheItem::Directory(Directory::new(
                name.to_os_string(),
                Some(attr),
                Some(vec![]),
            ));
            cache.put_new(path, item).await;
        }

        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn rename<P: AsRef<Path> + Debug>(
        &self,
        old_path: P,
        new_path: P,
        flags: RenameFlags,
    ) -> Result<()> {
        let old_path_str = path_to_string(&old_path)?;
        let new_path_str = path_to_string(&new_path)?;

        self.remote_client
            .rename(&old_path_str, &new_path_str, flags)
            .await?;

        if let Some(cache) = &self.cache {
            match flags {
                f if f.contains(RenameFlags::EXCHANGE) => {
                    cache.remove(old_path).await;
                    cache.remove(new_path).await;
                }
                _ => {
                    let old_item = cache.remove(old_path).await;
                    if let Some(name) = new_path.as_ref().file_name()
                        && let Some(mut item) = old_item
                    {
                        item.rename(name.to_os_string());
                        cache.put_new(new_path, item).await;
                    }
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn remove<P: AsRef<Path> + Debug>(&self, path: P) -> Result<()> {
        let path_str = path_to_string(&path)?;

        self.remote_client.remove(&path_str).await?;
        if let Some(cache) = &self.cache {
            cache.remove(path).await;
        }
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::DEBUG), ret(level = Level::DEBUG))]
    pub async fn get_attributes<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Attributes> {
        let path = path.as_ref();

        if let Some(cache) = &self.cache
            && let Some(item) = cache.get(path).await
            && let Some(attr) = item.get_attributes()
        {
            // cache hit
            return Ok(attr);
        }

        let path_str = path_to_string(path)?;

        let attributes = self.remote_client.get_attributes(&path_str).await?;

        if let Some(cache) = &self.cache {
            cache_put_attr(cache, path, attributes).await;
        }
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
        let path_str = path_to_string(&path)?;

        let attributes = self
            .remote_client
            .set_attributes(&path_str, new_attributes)
            .await?;

        if let Some(cache) = &self.cache {
            cache_put_attr(cache, path, attributes).await;
        }
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn get_permissions<P: AsRef<Path> + Debug>(&self, path: P, mask: u32) -> Result<()> {
        let path_str = path_to_string(&path)?;

        self.remote_client.get_permissions(&path_str, mask).await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_fs_stats<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Stats> {
        let path_str = path_to_string(&path)?;

        let stats = self.remote_client.get_stats(&path_str).await?;
        Ok(stats)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
    ) -> Result<Vec<u8>> {
        let path_str = path_to_string(&path)?;

        let xattributes = self.remote_client.get_x_attributes(&path_str, name).await?;
        match xattributes {
            Some(xattr) => Ok(xattr.get()),
            None => Err(FsModelError::NoData(format!(
                "Xattribute '{}' not found for path '{:?}'",
                name,
                path.as_ref()
            ))),
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn set_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
        xattributes: &[u8],
        flags: u32,
        position: u32,
    ) -> Result<()> {
        let path_str = path_to_string(&path)?;

        self.remote_client
            .set_x_attributes(&path_str, name, xattributes)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attribute<P: AsRef<Path> + Debug>(&self, path: P) -> Result<Vec<String>> {
        let path_str = path_to_string(&path)?;

        let names = self.remote_client.list_x_attributes(&path_str).await?;
        Ok(names)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn remove_x_attributes<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        name: &str,
    ) -> Result<()> {
        let path_str = path_to_string(&path)?;

        self.remote_client
            .remove_x_attributes(&path_str, name)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_symlink<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        target: &str,
    ) -> Result<Attributes> {
        let path = path.as_ref();
        let path_str = path_to_string(path)?;

        let attributes = self.remote_client.create_symlink(&path_str, target).await?;

        if let (Some(name), Some(cache)) = (path.file_name(), &self.cache) {
            let item = CacheItem::SymLink(SymLink::new(
                name.to_os_string(),
                Some(attributes),
                Some(target.to_string()),
            ));
            cache.put_new(path, item).await;
        }
        Ok(attributes)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_symlink<P: AsRef<Path> + Debug>(&self, path: P) -> Result<String> {
        if let Some(cache) = &self.cache
            && let Some(CacheItem::SymLink(SymLink { target, .. })) = cache.get(path.as_ref()).await
            && let Some(target) = target
        {
            // cache hit
            return Ok(target);
        }

        // cache miss
        let path_str = path_to_string(&path)?;
        let target = self.remote_client.read_symlink(&path_str).await?;

        if let (Some(name), Some(cache)) = (path.as_ref().file_name(), &self.cache) {
            let item = CacheItem::SymLink(SymLink::new(
                name.to_os_string(),
                None,
                Some(target.clone()),
            ));
            cache.put(path, item, false).await;
        }

        Ok(target)
    }

    // TODO: remove the ret log
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn flush_write_buffer(&self) -> Result<()> {
        let (path_owned, offset, data_owned) = {
            let mut buffer = self.write_buffer.write().await;
            let (path, offset, data) = buffer.get_content();

            if data.is_empty() {
                return Ok(());
            }

            let path_owned = path.to_path_buf();
            let data_owned = data.to_vec();
            buffer.clean();

            (path_owned, offset, data_owned)
        };

        let path_str = path_to_string(&path_owned)?;

        self.remote_client
            .write_file(&path_str, offset, &data_owned)
            .await?;

        if let Some(cache) = &self.cache {
            cache_write_file(cache, &path_owned, offset, &data_owned, true).await;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn cache_invalidate<P: AsRef<Path> + Debug>(&self, path: P) {
        if let Some(cache) = &self.cache {
            cache.invalidate(path).await;
        }
    }
}

fn path_to_string<P: AsRef<Path>>(path: P) -> Result<String> {
    path.as_ref()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| FsModelError::InvalidInput("Path is not valid UTF-8".to_string()))
}
