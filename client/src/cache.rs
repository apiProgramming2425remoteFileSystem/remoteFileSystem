use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Instant, Duration};
use crate::fs_model::directory::Directory;
use crate::fs_model::file::File;
use crate::fs_model::FileAttr;
use crate::network::models::{ItemType, SerializableFSItem};

#[derive(Clone, Debug)]
pub enum CacheItem{
    File(File),
    Directory(Directory),
}

impl CacheItem{
    pub fn rename(&mut self, name: OsString){
        match self {
            CacheItem::File(file) => {file.name = name;},
            CacheItem::Directory(directory) => {directory.name = name;},
        }
    }

    pub fn get_attributes(&self) -> Option<FileAttr>{
        match self{
            CacheItem::File(file) => {file.attributes.clone()},
            CacheItem::Directory(directory) => {directory.attributes.clone()},
        }
    }
}

impl From<SerializableFSItem> for CacheItem {
    fn from(item: SerializableFSItem) -> Self {
        match item.item_type {
            ItemType::Directory => CacheItem::Directory(
                Directory::new(item.name.into(),
                               Some(item.attributes),
                               None)),
            ItemType::File => CacheItem::File(
                File::new(item.name.into(),
                          Some(item.attributes),
                          None))
        }
    }
}


#[derive(Debug)]
struct CacheEntry{
    pub item: CacheItem,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub access_count: u64,
}

impl CacheEntry{
    pub fn new(item: CacheItem) -> CacheEntry{
        CacheEntry{item, created_at: Instant::now(), last_accessed: Instant::now(), access_count: 0}
    }

    pub fn update(&mut self, new_item: CacheItem) {
        match (&mut self.item, new_item) {
            (CacheItem::File(old), CacheItem::File(new)) => {
                old.merge(new);
            }
            (CacheItem::Directory(old), CacheItem::Directory(new)) => {
                old.merge(new);
            }
            (_, replacer) => {
                self.item = replacer;
            }
        }
    }
}

#[derive(Debug)]
pub struct Cache {
    pub entries: RwLock<HashMap<PathBuf, CacheEntry>>,
    pub capacity: usize,
    pub ttl: Duration,
    pub use_ttl: bool,
    pub policy: CachePolicy,
    pub max_file_size: usize,
}

impl Cache {
    pub fn from_config(cfg: &CacheConfig) -> Option<Self> {
        if !cfg.enabled {
            return None;
        }

        Some(Self {
            entries: RwLock::new(HashMap::new()),
            capacity: cfg.capacity,
            ttl: cfg.ttl,
            use_ttl: cfg.use_ttl,
            policy: cfg.policy,
            max_file_size: cfg.max_size,
        })
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Option<CacheItem> {
        let mut map = self.entries.write().unwrap();
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

    pub fn put<P: AsRef<Path>>(&self, path: P, mut item: CacheItem) {
        if let CacheItem::File(File { ref mut content, .. }) = item {
            if let Some(content) = content {
                if content.len() > self.max_file_size {
                    content.clear();
                }
            }
        }

        let mut map = self.entries.write().unwrap();
        let key = path.as_ref();

        if let Some(entry) = map.get_mut(key) {
            let is_valid = !self.use_ttl || entry.created_at + self.ttl >= Instant::now();
            if is_valid {
                // just update old entry
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                entry.update(item);
                return;
            }
            map.remove(key);
        }

        // insert new entry
        if map.len() >= self.capacity {
            if let Some(victim) = self.select_victim(&map) {
                map.remove(&victim);
            }
        }
        map.insert(key.to_path_buf(), CacheEntry::new(item));
    }

    pub fn remove<P: AsRef<Path>>(&self, path: P) -> Option<CacheItem> {
        let mut map = self.entries.write().unwrap();
        map.remove(path.as_ref()).map(|CacheEntry { item, .. }| item)
    }

    pub fn add_child<P: AsRef<Path>>(&self, path: P, name: OsString){
        let mut map = self.entries.write().unwrap();
        let key = path.as_ref();
        let Some(entry) = map.get_mut(key) else {
            return;
        };
        let CacheItem::Directory(dir) = &mut entry.item else {
            return;
        };
        dir.add_child(name);
    }


    fn select_victim(&self, map: &HashMap<PathBuf, CacheEntry>) -> Option<PathBuf> {
        match self.policy {
            CachePolicy::Lru => {
                map.iter()
                    .min_by_key(|(_, entry)| entry.last_accessed)
                    .map(|(k, _)| k.clone())
            }
            CachePolicy::Lfu => {
                map.iter()
                    .min_by_key(|(_, entry)| entry.access_count)
                    .map(|(k, _)| k.clone())
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CachePolicy {
    Lru,
    Lfu,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub enabled: bool,
    pub use_ttl: bool,
    pub ttl: Duration,
    pub policy: CachePolicy,
    pub max_size: usize,
    pub capacity: usize,
}
