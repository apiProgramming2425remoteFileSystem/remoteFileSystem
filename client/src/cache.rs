use crate::config::cache::{CacheConfig, CachePolicy};
use crate::fs_model::{Attributes, Directory, File, FileType, SymLink};
use crate::network::models::{ItemType, SerializableFSItem};

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use tracing::{Level, instrument};

pub trait CacheableItem {
    fn rename(&mut self, name: OsString);
    fn get_attributes(&self) -> Option<Attributes>;

    fn invalidate_attributes(&mut self);
}

#[derive(Clone, Debug)]
pub enum CacheItem {
    File(File),
    SymLink(SymLink),
    Directory(Directory),
}

impl CacheableItem for CacheItem {
    fn rename(&mut self, name: OsString) {
        match self {
            CacheItem::File(item) => item.rename(name),
            CacheItem::SymLink(item) => item.rename(name),
            CacheItem::Directory(item) => item.rename(name),
        }
    }

    fn get_attributes(&self) -> Option<Attributes> {
        match self {
            CacheItem::File(item) => item.get_attributes(),
            CacheItem::SymLink(item) => item.get_attributes(),
            CacheItem::Directory(item) => item.get_attributes(),
        }
    }

    fn invalidate_attributes(&mut self) {
        match self {
            CacheItem::File(item) => item.invalidate_attributes(),
            CacheItem::SymLink(item) => item.invalidate_attributes(),
            CacheItem::Directory(item) => item.invalidate_attributes(),
        }
    }
}

impl From<SerializableFSItem> for CacheItem {
    fn from(item: SerializableFSItem) -> Self {
        match item.item_type {
            ItemType::Directory => CacheItem::Directory(Directory::new(
                item.name.into(),
                Some(item.attributes),
                None,
            )),
            ItemType::SymLink => {
                CacheItem::SymLink(SymLink::new(item.name.into(), Some(item.attributes), None))
            }
            ItemType::File => CacheItem::File(File::new(item.name.into(), Some(item.attributes))),
        }
    }
}

#[derive(Debug)]
pub struct CacheEntry {
    pub item: CacheItem,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub access_count: u64,
}

impl CacheEntry {
    pub fn new(item: CacheItem) -> CacheEntry {
        CacheEntry {
            item,
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 0,
        }
    }

    pub fn update(&mut self, new_item: CacheItem) {
        match (&mut self.item, new_item) {
            (CacheItem::File(old), CacheItem::File(new)) => {
                old.merge(new);
            }
            (CacheItem::Directory(old), CacheItem::Directory(new)) => {
                old.merge(new);
            }
            (CacheItem::SymLink(old), CacheItem::SymLink(new)) => {
                old.merge(new);
            }
            (_, replacer) => {
                self.item = replacer;
            }
        }
    }
}

pub struct Cache {
    pub entries: RwLock<HashMap<PathBuf, CacheEntry>>,
    pub capacity: usize,
    pub ttl: Duration,
    pub use_ttl: bool,
    pub policy: CachePolicy,
    pub max_size: usize,
}

fn parent_paths<P: AsRef<Path> + Debug>(path: P) -> Vec<PathBuf> {
    let mut parents = Vec::new();
    let mut current = path.as_ref().parent();

    while let Some(p) = current {
        parents.push(p.to_path_buf());
        current = p.parent();
    }

    parents
}

impl Cache {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn from_config(cfg: &CacheConfig) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }

        Some(Cache {
            entries: RwLock::new(HashMap::new()),
            capacity: cfg.capacity,
            ttl: Duration::from_secs(cfg.ttl),
            use_ttl: cfg.use_ttl,
            policy: cfg.policy,
            max_size: cfg.max_size,
        })
    }

    async fn invalidate_parents<P: AsRef<Path>>(&self, path: P) {
        let parents = parent_paths(path.as_ref());
        let mut map = self.entries.write().await;

        for p in parents {
            map.remove(&p);
        }
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub async fn get<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        let mut map = self.entries.write().await;
        let key = path.as_ref();

        let entry = map.get_mut(key)?;

        if self.use_ttl && entry.created_at + self.ttl < Instant::now() {
            map.remove(key);
            return None;
        }

        entry.last_accessed = Instant::now();
        entry.access_count += 1;

        Some(entry.item.clone())
    }

    #[instrument(skip(self))]
    pub async fn put<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        item: CacheItem,
        invalidate_attributes: bool,
    ) {
        let mut map = self.entries.write().await;
        let key = path.as_ref();

        if let Some(entry) = map.get_mut(key) {
            let is_valid = !self.use_ttl || entry.created_at + self.ttl >= Instant::now();
            if is_valid {
                // just update old entry
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                entry.update(item);
                if invalidate_attributes {
                    entry.item.invalidate_attributes();
                }
                return;
            }
            map.remove(key);
        }

        // insert new entry
        if map.len() >= self.capacity
            && let Some(victim) = self.select_victim(&map)
        {
            map.remove(&victim);
        }
        map.insert(key.to_path_buf(), CacheEntry::new(item));
    }

    #[instrument(skip(self))]
    pub async fn put_new<P: AsRef<Path> + Debug>(&self, path: P, item: CacheItem) {
        self.invalidate_parents(&path).await;

        let mut map = self.entries.write().await;
        let key = path.as_ref().to_path_buf();

        if map.len() >= self.capacity
            && let Some(victim) = self.select_victim(&map)
        {
            map.remove(&victim);
        }

        map.insert(key, CacheEntry::new(item));
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub async fn remove<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        let removed = {
            let mut map = self.entries.write().await;
            map.remove(path.as_ref()).map(|e| e.item)
        };
        self.invalidate_parents(path).await;
        removed
    }

    #[instrument(skip(self))]
    pub async fn invalidate<P: AsRef<Path> + Debug>(&self, path: P) {
        let mut map = self.entries.write().await;
        map.remove(path.as_ref());
    }

    fn select_victim(&self, map: &HashMap<PathBuf, CacheEntry>) -> Option<PathBuf> {
        match self.policy {
            CachePolicy::Lru => map
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed)
                .map(|(k, _)| k.clone()),
            CachePolicy::Lfu => map
                .iter()
                .min_by_key(|(_, entry)| entry.access_count)
                .map(|(k, _)| k.clone()),
        }
    }
}

#[instrument(skip(cache, data))]
pub async fn cache_write_file<P: AsRef<Path> + Debug>(
    cache: &Cache,
    path: P,
    offset: usize,
    data: &[u8],
    invalidate_attributes: bool,
) {
    if let Some(name) = path.as_ref().file_name() {
        let mut file = File::new(name.to_os_string(), None);
        file.write_content(offset, data);
        cache
            .put(
                path.as_ref().to_path_buf(),
                CacheItem::File(file),
                invalidate_attributes,
            )
            .await;
    }
}

#[instrument(skip(cache))]
pub async fn cache_put_attr<P: AsRef<Path> + Debug>(
    cache: &Cache,
    path: P,
    attributes: Attributes,
) {
    if let Some(name) = path.as_ref().file_name() {
        let item = match attributes.kind {
            FileType::Directory => {
                CacheItem::Directory(Directory::new(name.to_os_string(), Some(attributes), None))
            }
            FileType::RegularFile => {
                CacheItem::File(File::new(name.to_os_string(), Some(attributes)))
            }
            FileType::Symlink => {
                CacheItem::SymLink(SymLink::new(name.to_os_string(), Some(attributes), None))
            }
            _ => CacheItem::File(File::new(name.to_os_string(), Some(attributes))),
        };

        cache.put(path.as_ref().to_path_buf(), item, false).await;
    }
}

impl Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Ok(map) = self.entries.try_read() else {
            return write!(f, "--");
        };
        let mut result = String::from("\n");
        for key in map.keys() {
            result += &format!("{:?}", key.display());
            result += " ";
            if let Some(entry) = map.get(key) {
                result += &format!("{:?}", entry);
            }
            result += "\n";
        }
        write!(f, "{}", result)
    }
}
