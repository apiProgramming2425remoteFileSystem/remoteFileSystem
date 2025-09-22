use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, RwLock, Weak};
use walkdir::WalkDir;

pub enum FSItem {
    File(File),
    Directory(Directory),
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

    pub fn get_children(&self) -> Option<&Vec<FSNode>> {
        match self {
            FSItem::Directory(d) => Some(&d.children),
            _ => None,
        }
    }

    // can be called only if you are sure that self is a directory
    pub fn add(&mut self, item: FSNode) {
        match self {
            FSItem::Directory(d) => {
                d.children.push(item);
            }
            _ => panic!("Cannot add item to non-directory"),
        }
    }

    pub fn remove(&mut self, name: &str) {
        match self {
            FSItem::Directory(d) => {
                d.children
                    .retain(|child| child.read().unwrap().name() != name);
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
    pub fn abs_path(&self) -> String {
        let mut parts = vec![];
        let mut current = self.parent().upgrade();

        while let Some(node) = current {
            let name = node.read().unwrap().name().to_string();
            parts.insert(0, name);
            current = node.read().unwrap().parent().upgrade();
        }

        if parts.len() < 2 {
            return "/".to_string();
        } else {
            return parts.join("/");
        }
    }
}

type FSItemCell = RwLock<FSItem>;
pub(crate) type FSNode = Arc<FSItemCell>;
type FSNodeWeak = Weak<FSItemCell>;

pub struct File {
    name: String,
    pub(crate) content: Vec<u8>,
    size: usize,
    parent: FSNodeWeak,
}

impl File {
    fn write_at(&mut self, data: &[u8], offset: usize) -> Result<(), String> {
        let new_len = offset + data.len();

        if self.content.len() < new_len {
            self.content.resize(new_len, 0);
        }

        self.content[offset..new_len].copy_from_slice(data);
        self.size = self.content.len();

        Ok(())
    }

    pub fn read_from(&self, offset: usize) -> Result<Vec<u8>, String>{
        // This function works only conidering a fictitious file-system
        let mut result = Vec::<u8>::new();
        self.content[offset..].clone_into(&mut result);
        Ok(result.clone())
    }
}

pub struct Directory {
    name: String,
    parent: FSNodeWeak,
    children: Vec<FSNode>,
}

pub struct FileSystem {
    real_path: String, // the real path of the file system
    root: FSNode,
    current: FSNode,
    side_effects: bool, // enable / disable side effects on the file system
}

impl FileSystem {
    pub fn new() -> Self {
        let root = Arc::new(RwLock::new(FSItem::Directory(Directory {
            name: "".to_string(),
            parent: Weak::new(),
            children: vec![],
        })));

        FileSystem {
            real_path: ".".to_string(),
            root: root.clone(),
            current: root,
            side_effects: false,
        }
    }

    pub fn from_file_system(base_path: &str) -> Self {
        let mut fs = FileSystem::new();
        fs.set_real_path(base_path);

        let wdir = WalkDir::new(base_path);
        for entry in wdir.into_iter().filter(|e| e.is_ok()).map(|e| e.unwrap()) {
            // full fs path
            let _entry_path = entry.path().to_str().unwrap();
            let entry_path = PathBuf::from(_entry_path);

            // remove base path, get relative path
            let rel_path = entry_path.strip_prefix(base_path).unwrap();

            // split path in head / tail
            let head = if let Some(parent) = rel_path.parent() {
                "/".to_string() + parent.to_str().unwrap()
            } else {
                "/".to_string()
            };
            let name = entry_path.file_name().unwrap().to_str().unwrap();

            if entry_path.is_dir() {
                fs.make_dir(&head, name).unwrap();
            } else if entry_path.is_file() {
                fs.make_file(&head, name).unwrap();
            }
        }

        fs
    }

    pub fn set_real_path(&mut self, path: &str) {
        self.real_path = path.to_string();
    }

    fn make_real_path(&self, node: FSNode) -> String {
        let mut abs_path = node.read().unwrap().abs_path();
        while abs_path.starts_with("/") {
            abs_path = abs_path[1..].to_string();
        }
        let real_path = PathBuf::from(&self.real_path)
            .join(&abs_path)
            .join(node.read().unwrap().name());

        return real_path.to_str().unwrap().to_string();
    }

    fn split_path(path: &str) -> Vec<&str> {
        path.split('/').filter(|&t| t != "").collect()
    }

    pub fn find(&self, path: &str) -> Option<FSNode> {
        self.find_full(path, None)
    }

    // find using either absolute or relative path
    pub fn find_full(&self, path: &str, base: Option<&str>) -> Option<FSNode> {
        let parts = FileSystem::split_path(path);

        let mut current = if path.starts_with('/') {
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
                    if part == "." {
                        current.clone()
                    } else if part == ".." {
                        d.parent.upgrade().unwrap()
                    } else {
                        let item = d
                            .children
                            .iter()
                            .find(|&child| child.read().unwrap().name() == part);

                        if let Some(item) = item {
                            item.clone()
                        } else {
                            return None;
                        }
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

    pub fn change_dir(&mut self, path: &str) -> Result<(), String> {
        let node = self.find(path);
        if let Some(n) = node {
            self.current = n;
            Ok(())
        } else {
            Err(format!("Directory {} not found", path))
        }
    }

    pub fn make_dir(&mut self, path: &str, name: &str) -> Result<(), String> {
        if let Some(node) = self.find(path) {
            if self.side_effects {
                // create the directory on the file system
                let real_path = self.make_real_path(node.clone());
                let target = PathBuf::from(&real_path).join(name);
                // if it fails for some reason just return an error with "?"
                fs::create_dir(&target).map_err(|e| e.to_string())?;
            }

            let new_dir = FSItem::Directory(Directory {
                name: name.to_string(),
                parent: Arc::downgrade(&node),
                children: vec![],
            });

            let new_node = Arc::new(RwLock::new(new_dir));
            node.write().unwrap().add(new_node.clone());

            Ok(())
        } else {
            return Err(format!("Directory {} not found", path));
        }
    }

    pub fn make_file(&mut self, path: &str, name: &str) -> Result<(), String> {
        if let Some(node) = self.find(path) {
            if self.side_effects {
                // create the file on the file system
                let real_path = self.make_real_path(node.clone());
                let target = PathBuf::from(&real_path).join(name);
                fs::File::create(&target).map_err(|e| e.to_string())?;
            }

            let new_file = FSItem::File(File {
                name: name.to_string(),
                content: Vec::new(),
                size: 0,
                parent: Arc::downgrade(&node),
            });

            let new_node = Arc::new(RwLock::new(new_file));
            node.write().unwrap().add(new_node.clone());
            Ok(())
        } else {
            return Err(format!("Directory {} not found", path));
        }
    }

    pub fn rename(&self, path: &str, new_name: &str) -> Result<(), String> {
        let node = self.find(path);
        if let Some(n) = node {
            if self.side_effects {
                let real_path = self.make_real_path(n.clone());
                // dest
                let mut parts = real_path.split("/").collect::<Vec<&str>>();
                parts.pop();
                parts.push(new_name); // remove the last part (the file name)
                let new_path = parts.join("/");
                fs::rename(&real_path, &new_path).map_err(|e| e.to_string())?;
            }

            n.write().unwrap().set_name(new_name);
            Ok(())
        } else {
            Err(format!("Item {} not found", path))
        }
    }

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

    pub fn set_side_effects(&mut self, side_effects: bool) {
        self.side_effects = side_effects;
    }

    pub fn write_file(&self, path: &str, data: &[u8], offset: usize) -> Result<(), String> {
        if let Some(node) = self.find(&path) {
            let mut item = node.write().unwrap();
            match item.deref_mut() {
                FSItem::Directory(_) => Err("Path is a directory, cannot write data".to_string()),
                FSItem::File(file_mut) => {
                    if self.side_effects {
                        // write the file on the file system
                        let real_path = self.make_real_path(node.clone());
                        let target = PathBuf::from(&real_path);

                        let mut f = fs::OpenOptions::new()
                            .write(true)
                            .open(&target)
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
                    file_mut.write_at(data, offset)?;
                    Ok(())
                }
            }
        } else {
            Err("Path not found".to_string())
        }
    }

    pub fn read_file(&self, path: &str, offset: usize) -> Result<Vec<u8>, String>{
        if let Some(node) = self.find(&path) {
            let mut item = node.read().unwrap();
            match item.deref() {
                FSItem::Directory(_) => Err("Path is a directory, cannot read data.".to_string()),
                FSItem::File(file_mut) => {
                    if self.side_effects {
                        // read the file from the real file system
                        let real_path = self.make_real_path(node.clone());
                        let target = PathBuf::from(&real_path);

                        let mut f = fs::OpenOptions::new()
                            .read(true)
                            .open(&target)
                            .map_err(|e| format!("Failed to open file: {}", e))?;

                        // Seek to offset
                        f.seek(SeekFrom::Start(offset as u64))
                            .map_err(|e| format!("Failed to seek: {}", e))?;

                        let mut buffer = Vec::<u8>::new();
                        let bytes_read = f.read(&mut buffer)
                            .map_err(|e| format!("Failed to read: {}", e))?;
                        buffer.truncate(bytes_read);
                        return Ok(buffer);
                    }
                    // In-memory read
                    let start = offset;
                    let end = std::cmp::min(start, file_mut.content.len());
                    let data = file_mut.content[start..].to_vec();
                    Ok(data.clone())
                }
            }
        } else {
            Err("Path not found.".to_string())
        }
    }
}
