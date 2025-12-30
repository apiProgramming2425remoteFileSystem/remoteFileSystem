use crate::config::{CacheConfig, CachePolicy};
use crate::fs_model::Attributes;
use crate::fs_model::directory::Directory;
use crate::fs_model::file::{File, MAX_PAGES, PAGE_SIZE};
use crate::fs_model::sym_link::SymLink;
use crate::network::models::{ItemType, SerializableFSItem};

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tracing::{Level, instrument};

// REVIEW: add a trait for Cacheable items in fs_model?
// like CacheableItem with methods like merge, get_attributes, invalidate_attributes, rename
// then implement it for File, SymLink, Directory and CacheItem can just delegate to those methods

#[derive(Clone, Debug)]
pub enum CacheItem {
    File(File),
    SymLink(SymLink),
    Directory(Directory),
}

impl CacheItem {
    pub fn rename(&mut self, name: OsString) {
        match self {
            CacheItem::File(file) => {
                file.name = name;
            }
            CacheItem::SymLink(link) => {
                link.name = name;
            }
            CacheItem::Directory(directory) => {
                directory.name = name;
            }
        }
    }

    pub fn get_attributes(&self) -> Option<Attributes> {
        match self {
            CacheItem::File(file) => file.attributes.clone(),
            CacheItem::SymLink(link) => link.attributes.clone(),
            CacheItem::Directory(directory) => directory.attributes.clone(),
        }
    }

    pub fn invalidate_attributes(&mut self) {
        match self {
            CacheItem::File(file) => {
                file.attributes = None;
            }
            CacheItem::SymLink(link) => {
                link.attributes = None;
            }
            CacheItem::Directory(directory) => {
                directory.attributes = None;
            }
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
    pub max_file_size: usize,
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
    pub fn from_config(cfg: &CacheConfig) -> Option<Arc<Self>> {
        if !cfg.enabled {
            return None;
        }

        // REVIEW: move this to a fs_model configurator?
        MAX_PAGES
            .set(cfg.max_size / PAGE_SIZE)
            .expect("MAX_PAGES already set");

        Some(Arc::new(Cache {
            entries: RwLock::new(HashMap::new()),
            capacity: cfg.capacity,
            ttl: Duration::from_secs(cfg.ttl),
            use_ttl: cfg.use_ttl,
            policy: cfg.policy,
            max_file_size: cfg.max_size,
        }))
    }

    fn invalidate_parents<P: AsRef<Path>>(&self, path: P) {
        let parents = parent_paths(path.as_ref());
        let Ok(mut map) = self.entries.write() else {
            return;
        };

        for p in parents {
            map.remove(&p);
        }
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn get<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        let Ok(mut map) = self.entries.write() else {
            return None;
        };
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
    pub fn put<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        item: CacheItem,
        invalidate_attributes: bool,
    ) {
        let Ok(mut map) = self.entries.write() else {
            return;
        };
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
        if map.len() >= self.capacity {
            if let Some(victim) = self.select_victim(&map) {
                map.remove(&victim);
            }
        }
        map.insert(key.to_path_buf(), CacheEntry::new(item));
    }

    #[instrument(skip(self))]
    pub fn put_new<P: AsRef<Path> + Debug>(&self, path: P, item: CacheItem) {
        self.invalidate_parents(&path);

        let Ok(mut map) = self.entries.write() else {
            return;
        };
        let key = path.as_ref().to_path_buf();

        if map.len() >= self.capacity {
            if let Some(victim) = self.select_victim(&map) {
                map.remove(&victim);
            }
        }

        map.insert(key, CacheEntry::new(item));
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn remove<P: AsRef<Path> + Debug>(&self, path: P) -> Option<CacheItem> {
        let removed = {
            let Ok(mut map) = self.entries.write() else {
                return None;
            };
            map.remove(path.as_ref()).map(|e| e.item)
        };
        self.invalidate_parents(path);
        removed
    }

    #[instrument(skip(self))]
    pub fn invalidate<P: AsRef<Path> + Debug>(&self, path: P) {
        let Ok(mut map) = self.entries.write() else {
            return;
        };
        map.remove(path.as_ref()).map(|e| e.item);
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
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

impl Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Ok(map) = self.entries.write() else {
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
