use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, RwLock, Weak};

use tracing::{Level, instrument};
use walkdir::WalkDir;

// #[derive(Debug)]
pub struct File {
    name: String,
    content: Vec<u8>,
    size: usize,
    parent: FSNodeWeak,
}

// #[derive(Debug)]
pub struct Directory {
    name: String,
    parent: FSNodeWeak,
    children: HashMap<PathBuf, FSNode>,
}

#[derive(Debug)]
pub enum FSItem {
    File(File),
    Directory(Directory),
}

type FSItemCell = RwLock<FSItem>;

pub type FSNode = Arc<FSItemCell>;
type FSNodeWeak = Weak<FSItemCell>;

// #[derive(Debug)]
pub struct FileSystem {
    real_path: PathBuf, // the real path of the file system
    root: FSNode,
    current: FSNode,
    side_effects: bool, // enable / disable side effects on the file system
}

impl File {
    fn new(name: &str, content: &[u8], size: usize, parent: FSNodeWeak) -> Self {
        Self {
            name: name.to_string(),
            content: Vec::from(content),
            size,
            parent,
        }
    }

    fn write_at(&mut self, data: &[u8], offset: usize) -> Result<(), String> {
        let new_len = offset + data.len();

        if self.content.len() < new_len {
            self.content.resize(new_len, 0);
        }

        self.content[offset..new_len].copy_from_slice(data);
        self.size = self.content.len();

        Ok(())
    }

    fn read_from(&self, offset: usize) -> Result<Vec<u8>, String> {
        // This function works only conidering a fictitious file-system
        let mut result = Vec::<u8>::new();
        self.content[offset..].clone_into(&mut result);
        Ok(result.clone())
    }
}

impl Directory {
    fn new(name: &str, parent: FSNodeWeak) -> Self {
        Self {
            name: name.to_string(),
            parent,
            children: HashMap::new(),
        }
    }
}

impl FSItem {
    // These methods allow us to use an FSItem in a uniform way
    // regardless of its actual type.
    pub fn name(&self) -> &str {
        match self {
            FSItem::File(f) => &f.name,
            FSItem::Directory(d) => &d.name,
        }
    }

    pub fn parent(&self) -> FSNodeWeak {
        match self {
            FSItem::File(f) => f.parent.clone(),
            FSItem::Directory(d) => d.parent.clone(),
        }
    }

    pub fn get_children(&self) -> Option<Vec<FSNode>> {
        match self {
            FSItem::Directory(d) => Some(d.children.iter().map(|(_, n)| n.clone()).collect()),
            _ => None,
        }
    }

    // can be called only if you are sure that self is a directory
    pub fn add(&mut self, item: FSNode) {
        match self {
            FSItem::Directory(d) => {
                d.children
                    .insert(PathBuf::from(item.clone().read().unwrap().name()), item);
            }
            _ => panic!("Cannot add item to non-directory"),
        }
    }

    pub fn remove(&mut self, name: &str) {
        match self {
            FSItem::Directory(d) => {
                d.children.remove(&PathBuf::from(name));
            }
            _ => panic!("Cannot remove item from non-directory"),
        }
    }

    pub fn set_name(&mut self, name: &str) {
        match self {
            FSItem::File(f) => f.name = name.to_owned(),
            FSItem::Directory(d) => d.name = name.to_owned(),
        }
    }

    // return the absolute path of the item (of the parent)
    pub fn abs_path(&self) -> PathBuf {
        let mut parts = vec![];
        let mut current = self.parent().upgrade();

        while let Some(node) = current {
            let name = node.read().unwrap().name().to_string();
            parts.insert(0, name);
            current = node.read().unwrap().parent().upgrade();
        }

        if parts.len() < 2 {
            return PathBuf::from("/");
        }

        parts.iter().collect()
    }
}

impl FileSystem {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(root: &str, real: bool) -> Self {
        let root_fs = Arc::new(RwLock::new(FSItem::Directory(Directory::new(
            root,
            Weak::new(),
        ))));

        FileSystem {
            real_path: PathBuf::from(root),
            root: root_fs.clone(),
            current: root_fs,
            side_effects: real,
        }
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub fn from_file_system(base_path: &str, real: bool) -> Self {
        let mut fs = FileSystem::new(base_path, real);

        let wdir = WalkDir::new(base_path);
        for entry in wdir
            .into_iter()
            .skip(1) // skip the root
            .filter(|e| e.is_ok())
            .map(|e| e.unwrap())
        {
            // full fs path
            let _entry_path = entry.path().to_str().unwrap();
            let entry_path = PathBuf::from(_entry_path);

            // remove base path, get relative path
            let rel_path = entry_path.strip_prefix(base_path).unwrap();

            // split path in head / tail
            let mut head = PathBuf::from("/");
            if let Some(parent) = rel_path.parent() {
                head = head.join(parent);
            }

            let name = entry_path.file_name().unwrap().to_str().unwrap();

            if entry_path.is_dir() {
                fs.make_dir(&head.to_str().unwrap(), name).unwrap();
            } else if entry_path.is_file() {
                fs.make_file(&head.to_str().unwrap(), name).unwrap();
            }
        }

        fs
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn set_real_path(&mut self, path: &str) {
        self.real_path = PathBuf::from(path).canonicalize().unwrap();
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn set_side_effects(&mut self, side_effects: bool) {
        self.side_effects = side_effects;
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn make_real_path(&self, node: FSNode) -> PathBuf {
        let mut abs_path = node.read().unwrap().abs_path();

        abs_path = abs_path
            .components()
            .filter(|c| *c != std::path::Component::RootDir)
            .collect();

        PathBuf::from(&self.real_path)
            .join(&abs_path)
            .join(node.read().unwrap().name())
    }

    #[instrument(ret(level = Level::TRACE))]
    fn split_path(path: &str) -> Vec<PathBuf> {
        let path = PathBuf::from(path);
        path.components()
            .skip(1) // skip the RootDir
            .map(|p| PathBuf::from(p.as_os_str()))
            .collect()
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find(&self, path: &str) -> Option<FSNode> {
        self.find_full(path, None)
    }

    // find using either absolute or relative path
    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find_full(&self, path: &str, base: Option<&str>) -> Option<FSNode> {
        let parts = FileSystem::split_path(path);

        let path = PathBuf::from(path);

        let mut current = if path.has_root() {
            self.root.clone()
        } else {
            if let Some(base) = base {
                // if we can't find the base, return None
                self.find(base)?
            } else {
                self.current.clone()
            }
        };

        for part in parts {
            let next_node = match current.read().unwrap().deref() {
                FSItem::Directory(d) => {
                    if part == PathBuf::from(".") {
                        current.clone()
                    } else if part == PathBuf::from("..") {
                        d.parent.upgrade().unwrap()
                    } else {
                        let Some(item) = d.children.get(&part) else {
                            return None;
                        };

                        item.clone()
                    }
                }
                FSItem::File(_) => {
                    return None;
                }
            };
            current = next_node;
        }
        Some(current)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn change_dir(&mut self, path: &str) -> Result<(), String> {
        let node = self.find(path);
        if let Some(n) = node {
            self.current = n;
            Ok(())
        } else {
            Err(format!("Directory {} not found", path))
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_dir(&mut self, path: &str, name: &str) -> Result<(), String> {
        if let Some(node) = self.find(path) {
            if self.side_effects {
                // create the directory on the file system
                let target = self.make_real_path(node.clone()).join(name);
                // if it fails for some reason just return an error with "?"
                fs::create_dir(&target).map_err(|e| e.to_string())?;
            }

            let new_dir = FSItem::Directory(Directory::new(name, Arc::downgrade(&node)));

            let new_node = Arc::new(RwLock::new(new_dir));
            node.write().unwrap().add(new_node.clone());

            Ok(())
        } else {
            return Err(format!("Directory {} not found", path));
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_file(&mut self, path: &str, name: &str) -> Result<(), String> {
        if let Some(node) = self.find(path) {
            if self.side_effects {
                // create the file on the file system
                let target = self.make_real_path(node.clone()).join(name);
                fs::File::create(&target).map_err(|e| e.to_string())?;
            }

            let new_file = FSItem::File(File::new(name, &[], 0, Arc::downgrade(&node)));

            let new_node = Arc::new(RwLock::new(new_file));
            node.write().unwrap().add(new_node.clone());
            Ok(())
        } else {
            return Err(format!("Directory {} not found", path));
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn rename(&self, path: &str, new_name: &str) -> Result<(), String> {
        let node = self.find(path);
        if let Some(n) = node {
            if self.side_effects {
                let real_path = self.make_real_path(n.clone());
                let new_path = real_path.with_file_name(new_name);

                fs::rename(&real_path, &new_path).map_err(|e| e.to_string())?;
            }

            n.write().unwrap().set_name(new_name);
            Ok(())
        } else {
            Err(format!("Item {} not found", path))
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn delete(&self, path: &str) -> Result<(), String> {
        let node = self.find(path);
        if let Some(n) = node {
            // true when we will work on a real file system
            if self.side_effects {
                match n.read().unwrap().deref() {
                    FSItem::File(_) => {
                        let real_path = self.make_real_path(n.clone());
                        fs::remove_file(&real_path).map_err(|e| e.to_string())?;
                    }
                    FSItem::Directory(_) => {
                        let real_path = self.make_real_path(n.clone());
                        fs::remove_dir_all(&real_path).map_err(|e| e.to_string())?;
                    }
                }
            }

            if let Some(parent) = n.read().unwrap().parent().upgrade() {
                parent.write().unwrap().remove(&n.read().unwrap().name());
            }
            Ok(())
        } else {
            Err(format!("Item {} not found", path))
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn write_file(&mut self, path: &str, data: &[u8], offset: usize) -> Result<(), String> {
        // Try to find node, or create file if not exists
        let node_opt = self.find(path).or_else(|| {
            let path_buf = PathBuf::from(path);
            let name = path_buf.file_name().and_then(|f| f.to_str())?;
            let dir = path_buf.parent().and_then(|p| p.to_str())?;

            self.make_file(dir, name).ok()?;
            self.find(path)
        });

        let node = node_opt.ok_or_else(|| "Path not found".to_string())?;

        let item = node.read().unwrap();
        match item.deref() {
            FSItem::Directory(_) => Err("Path is a directory, cannot write data".to_string()),
            FSItem::File(_) => {
                drop(item);

                if self.side_effects {
                    // write the file on the file system
                    let real_path = self.make_real_path(node.clone());
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .open(&real_path)
                        .map_err(|e| format!("Failed to open file: {}", e))?;

                    // Seek to offset
                    f.seek(SeekFrom::Start(offset as u64))
                        .map_err(|e| format!("Failed to seek: {}", e))?;
                    // Write data
                    f.write_all(data)
                        .map_err(|e| format!("Failed to write: {}", e))?;
                    // Rewind to start to read the updated file content
                    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                }

                // Update the in-memory file content under a write lock
                let mut item = node.write().unwrap();
                match item.deref_mut() {
                    FSItem::File(file_mut) => file_mut.write_at(data, offset),
                    _ => unreachable!(),
                }
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn read_file(&self, path: &str, offset: usize) -> Result<Vec<u8>, String> {
        if let Some(node) = self.find(&path) {
            let item = node.read().unwrap();
            match item.deref() {
                FSItem::Directory(_) => Err("Path is a directory, cannot read data.".to_string()),
                FSItem::File(file_mut) => {
                    if self.side_effects {
                        // read the file from the real file system
                        let real_path = self.make_real_path(node.clone());

                        let mut f = fs::OpenOptions::new()
                            .read(true)
                            .open(&real_path)
                            .map_err(|e| format!("Failed to open file: {}", e))?;

                        // Seek to offset
                        f.seek(SeekFrom::Start(offset as u64))
                            .map_err(|e| format!("Failed to seek: {}", e))?;

                        let mut buffer = Vec::<u8>::new();
                        let bytes_read = f
                            .read(&mut buffer)
                            .map_err(|e| format!("Failed to read: {}", e))?;
                        buffer.truncate(bytes_read);
                        return Ok(buffer);
                    }
                    // In-memory read
                    let data = file_mut.content[offset..].to_vec();
                    Ok(data.clone())
                }
            }
        } else {
            Err("Path not found.".to_string())
        }
    }

    pub fn move_node(&self, old_path: &str, new_path: &str) -> Result<(), ()> {
        let (old_parent_path, old_name) = match old_path.rsplit_once('/') {
            Some((p, name)) => (p, name),
            None => return Err(()),
        };
        let parent_old = match self.find(old_parent_path) {
            Some(n) => n,
            None => return Err(()),
        };
        let (new_parent_path, new_name) = match new_path.rsplit_once('/') {
            Some((p, name)) => (p, name),
            None => return Err(()),
        };

        if old_parent_path == new_parent_path {
            return match self.rename(old_path, new_name) {
                Ok(()) => Ok(()),
                Err(_) => Err(()),
            };
        }

        let parent_new = match self.find(new_parent_path) {
            Some(n) => n,
            None => return Err(()),
        };

        let mut parent_old_guard = parent_old.write().unwrap();
        let node_to_move = match parent_old_guard
            .get_children()
            .unwrap()
            .iter()
            .find(|child| child.read().unwrap().name() == old_name)
        {
            Some(node) => node.clone(),
            None => return Err(()),
        };

        parent_old_guard.remove(old_name);
        node_to_move.write().unwrap().set_name(&new_name);

        let mut parent_new_guard = parent_new.write().unwrap();
        parent_new_guard.add(node_to_move);

        Ok(())
    }
}

impl Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("File").field("name", &self.name).finish()
    }
}

impl Debug for Directory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Directory")
            .field("name", &self.name)
            .field("parent", &self.parent)
            .finish()
    }
}

impl Debug for FileSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSystem")
            .field("real_path", &self.real_path)
            .field("side_effects", &self.side_effects)
            .finish()?;

        writeln!(f, "\nRoot Directory:")?;
        draw_tree(f, &self.root, "")?;
        writeln!(f, "\nCurrent Directory:")?;
        draw_tree(f, &self.current, "")
    }
}

// Helper function to recursively write directory tree with branches
fn draw_tree(f: &mut std::fmt::Formatter<'_>, node: &FSNode, prefix: &str) -> std::fmt::Result {
    let node_guard = node.read().map_err(|_| std::fmt::Error)?;
    match &*node_guard {
        FSItem::File(file) => writeln!(f, "{:?}", file),
        FSItem::Directory(dir) => {
            writeln!(f, "{:?}", dir)?;
            let len = dir.children.len();
            for (i, child) in dir.children.iter().enumerate() {
                let (new_prefix, branch) = if i + 1 == len {
                    (format!("{}    ", prefix), "└── ")
                } else {
                    (format!("{}│   ", prefix), "├── ")
                };
                write!(f, "{}{}", prefix, branch)?;
                draw_tree(f, child.1, &new_prefix)?;
            }
            Ok(())
        }
    }
}
