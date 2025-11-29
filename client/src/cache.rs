use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Instant, Duration};
use crate::fs_model::directory::Directory;
use crate::fs_model::file::File;
use crate::network::models::{ItemType, SerializableFSItem};

#[derive(Clone, Debug)]
pub enum CacheItem{
    File(File),
    Directory(Directory),
}

impl From<SerializableFSItem> for CacheItem {
    fn from(item: SerializableFSItem) -> Self {
        match item.item_type {
            ItemType::Directory => CacheItem::Directory(Directory {
                name: item.name.into(),
                children: vec![],
                attributes: item.attributes,
                valid_children: false,
            }),
            ItemType::File => CacheItem::File(File {
                name: item.name.into(),
                content: vec![],
                attributes: item.attributes,
                valid_content: false,
            })
        }
    }
}


#[derive(Debug)]
struct CacheEntry{
    pub item: CacheItem,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub access_count: u64,
    pub ttl: Duration,
}

impl CacheEntry{
    pub fn new(item: CacheItem, ttl: Duration) -> CacheEntry{
        CacheEntry{item, created_at: Instant::now(), last_accessed: Instant::now(), access_count: 0, ttl}
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

        if self.use_ttl && entry.created_at + entry.ttl < Instant::now() {
            map.remove(key);
            return None;
        }

        entry.last_accessed = Instant::now();
        entry.access_count += 1;

        Some(entry.item.clone())
    }

    pub fn put<P: AsRef<Path>>(&self, path: P, mut item: CacheItem) {
        if let CacheItem::File(File { ref mut content, ref mut valid_content, .. }) = item {
            if content.len() > self.max_file_size {
                content.clear();
                *valid_content = false;
            }
        }

        let mut map = self.entries.write().unwrap();
        let key = path.as_ref().to_path_buf();

        if map.len() >= self.capacity {
            if let Some(victim) = self.select_victim(&map) {
                map.remove(&victim);
            }
        }

        let entry = CacheEntry::new(item, self.ttl);
        map.insert(key, entry);
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
