use std::fs::{self, OpenOptions, FileTimes};
#[cfg(target_family="unix")]
use std::os::unix::fs::PermissionsExt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, Weak};
use std::time::SystemTime;

use crate::models::{Permission, SetAttr, Stats, Timestamp};
use walkdir::WalkDir;

use crate::models::{FileAttr, FileType};
#[cfg(target_family="unix")]
use nix::sys::statvfs::Statvfs;
#[cfg(target_family="unix")]
use nix::sys::statvfs::statvfs;

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
    attributes: FileAttr,
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

    pub fn read_from(&self, offset: usize) -> Result<Vec<u8>, String> {
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
    attributes: FileAttr,
}

pub struct FileSystem {
    real_path: String, // the real path of the file system
    root: FSNode,
    current: FSNode,
    side_effects: bool, // enable / disable side effects on the file system
}

fn check_permission(owner_uid: u32, owner_gid:u32, uid: u32, gid: u32) -> bool{
    owner_uid == uid || uid == 0 || owner_gid == gid
}

fn get_attributes_by_path(path: &Path) -> Result<FileAttr, String>{
    match fs::metadata(path) {
        Ok(object) => {
            let nlink = 1;

            let kind = if object.is_dir(){
                FileType::Directory
            }else if object.is_file(){
                FileType::RegularFile
            }else{
                FileType::Symlink
            };


            let attributes = FileAttr {
                size: object.len(),
                blocks: 0, // ? eventualmente modificare ?
                atime: Timestamp::from(object.accessed().map_err(|_|"Error in timestamp convertion.")?),
                mtime: Timestamp::from(object.modified().map_err(|_|"Error in timestamp convertion.")?),
                ctime: Timestamp::from(SystemTime::now()), 
                crtime: Timestamp::from(SystemTime::now()),
                kind: kind,
                perm: Permission::try_from(0o755 as u16).map_err(|_|"Error in permission convertion.")?, // retrieve from db
                nlink: nlink,
                uid: 0,       // retrieve from db
                gid: 0,       // retrieve from db
                rdev: 0, // device ID of a special file in Unix-like operating systems, indicating the device associated with a file
                blksize: 0, // ? eventualmente modificare ?
                flags: 0, // macOS only
            };
            return Ok(attributes);
        }
        Err(_) => {
            return Err("Error while obtaining file metadata.".to_string());
        }
    }
}

impl FileSystem {
    pub fn new() -> Self {
        let root = Arc::new(RwLock::new(FSItem::Directory(Directory {
            name: "".to_string(),
            parent: Weak::new(),
            children: vec![],
            attributes: FileAttr {
                size: 0,
                blocks: 0,
                atime: SystemTime::now().into(),
                mtime: SystemTime::now().into(),
                ctime: SystemTime::now().into(),
                crtime: SystemTime::now().into(),
                perm: Permission::try_from(0o755 as u16).unwrap(),
                kind: FileType::Directory,
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: 4096,
                flags: 1,
            },
        })));

        FileSystem {
            real_path: ".".to_string(),
            root: root.clone(),
            current: root,
            side_effects: true,
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
                attributes: FileAttr {
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::now().into(),
                    mtime: SystemTime::now().into(),
                    ctime: SystemTime::now().into(),
                    crtime: SystemTime::now().into(),
                    kind: FileType::Directory,
                    perm: Permission::try_from(0o755 as u16).unwrap(),
                    nlink: 2,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 4096,
                    flags: 1,
                },
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
                attributes: FileAttr {
                    size: 0,
                    blocks: 0,
                    atime: SystemTime::now().into(),
                    mtime: SystemTime::now().into(),
                    ctime: SystemTime::now().into(),
                    crtime: SystemTime::now().into(),
                    kind: FileType::RegularFile,
                    perm: Permission::try_from(0o755 as u16).unwrap(),
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: 4096,
                    flags: 1,
                },
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

    pub fn write_file(&mut self, path: &str, data: &[u8], offset: usize) -> Result<(), String> {
        let node_opt = self.find(path).or_else(|| {
            let x = PathBuf::from(path);
            let name = x.file_name().and_then(|f| f.to_str())?;
            let dir = x.parent().and_then(|p| p.to_str())?;
            match self.make_file(dir, name) {
                Ok(_) => self.find(path),
                Err(_) => None,
            }
        });
        match node_opt {
            None => Err("Path not found".to_string()),
            Some(node) => {
                let mut item = node.write().unwrap();
                match item.deref_mut() {
                    FSItem::Directory(_) => {
                        Err("Path is a directory, cannot write data".to_string())
                    }
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
            }
        }
    }

    pub fn read_file(&self, path: &str, offset: usize) -> Result<Vec<u8>, String> {
        if let Some(node) = self.find(&path) {
            let item = node.read().unwrap();
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
        // avoid moving a dir in its children (mv a/b a/b/c/d)
        if new_path == old_path || new_path.starts_with(&format!("{old_path}/")) {
            return Err(());
        }

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

    /* IMPLEMENT PERMISSION MANAGEMENT VIA DB */
    pub fn get_attributes(&self, path: &str) -> Result<FileAttr, String> {
        if let Some(node) = self.find(path) {
            if self.side_effects{
                let real_path = self.make_real_path(node.clone());
                let target = Path::new(&real_path);

                return get_attributes_by_path(target);
            }

            /* DA ELIMINARE UNA VOLTA IN perm_storage */
            let item = node.read().unwrap();
            match item.deref() {
                FSItem::Directory(dir) => {
                    Ok(dir.attributes.clone())
                },
                FSItem::File(f) => {
                        Ok(f.attributes.clone())
                },
                _ => Err("No file or directory in this path".to_string()),
            }
            /* -------------------------------------- */
        }else{
            Err("Invalid path".to_string())
        }
    }

    /* Inutile se lookup = getattr
    pub fn resolve_child(&self, path: &str) -> Result<FileAttr, String>{
        /* Suggerimento Copilot */
        let parent_path = self.inode_map.get(&parent)?;
        let child_path = format!("{}/{}", parent_path, name.to_string_lossy());

        // Interroga il backend remoto
        let metadata = self.client.get_metadata(&child_path).ok()?;

        // Genera inode se non esiste
        let inode = *self.path_map.entry(child_path.clone()).or_insert_with(|| {
            let id = self.next_inode;
            self.next_inode += 1;
            self.inode_map.insert(id, child_path.clone());
            id
        });

        let attr = FileAttr {
            ino: inode,
            size: metadata.size,
            blocks: (metadata.size + 511) / 512,
            atime: metadata.atime,
            mtime: metadata.mtime,
            ctime: metadata.ctime,
            crtime: metadata.ctime,
            kind: metadata.kind,
            perm: metadata.perm,
            nlink: 1,
            uid: metadata.uid,
            gid: metadata.gid,
            rdev: 0,
            flags: 0,
            blksize: 512,
        };

        Some((inode, attr))
    }*/

    /* IMPLEMENT PERMISSION MANAGEMENT VIA DB */
    pub fn set_attributes(&self, path: &str, uid: u32, gid: u32, new_attributes: SetAttr) -> Result<FileAttr, String>{
        if let Some(node) = self.find(path){

            if self.side_effects {
                let real_path = self.make_real_path(node.clone());
                let target = Path::new(&real_path);

                match fs::metadata(target) {
                    Ok(object) => {
                        let owner_uid = 0; // to substitute with call to db
                        let owner_gid = 0; // to substitute with call to db

                        /* ALWAYS ALLOWED CHANGES */
                        let new_times = FileTimes::new();
                        
                        // access time
                        if new_attributes.atime.is_some(){
                            let new_atime = new_attributes.atime.unwrap();
                            new_times.set_accessed(SystemTime::from(new_atime));
                        }
                        // modification time
                        if new_attributes.mtime.is_some(){
                            let new_mtime = new_attributes.mtime.unwrap();
                            new_times.set_modified(SystemTime::from(new_mtime));
                        }
                        // creation time is automatically managed by kernel

                        /* CHANGES ALLOWED ONLY IF USER HAS PERMISSION */
                        let has_permission = check_permission(owner_uid, owner_gid, uid, gid);

                        if new_attributes.mode.is_some(){
                            if has_permission == false{
                                return Err(String::from("User has not priviledge to do this change."));
                            }
                            let new_mode = new_attributes.mode.unwrap();
                            // update info on db
                        }

                        if new_attributes.size.is_some(){
                            if has_permission == false{
                                return Err(String::from("User has not priviledge to do this change."));
                            }
                            let new_size = new_attributes.size.unwrap();
                            let file = OpenOptions::new().write(true).open(target).map_err(|_|String::from("Impossible to open file."))?;
                            file.set_len(new_size);
                        }

                        if new_attributes.uid.is_some() || new_attributes.gid.is_some(){
                            if has_permission == false{
                                return Err(String::from("User has not priviledge to do this change."));
                            }
                            let new_uid = if let Some(new) = new_attributes.uid{
                                new
                            }else{
                                uid
                            };

                            let new_gid = if let Some(new) = new_attributes.gid{
                                new
                            }else{
                                gid
                            };

                            // update db
                        }

                        // returns new attributes
                        let attributes = self.get_attributes(path).unwrap();
                        return Ok(attributes);
                        },
                        Err(_) => {
                            return Err("Error while obtaining file metadata.".to_string());
                        }
                    }
                }

            /* DA ELIMINARE UNA VOLTA IN perm_storage */
            let item = node.read().unwrap();
            match item.deref() {
                FSItem::Directory(dir) => {
                    Ok(dir.attributes.clone())
                },
                FSItem::File(f) => {
                        Ok(f.attributes.clone())
                },
                _ => Err("No file or directory in this path".to_string()),
            }
            /* -------------------------------------- */
        }else{
            Err("Invalid path".to_string())
        }
    }

    /* IMPLEMENT PERMISSION MANAGEMENT VIA DB */
    pub fn get_permissions(&self, path: &str) -> Result<u32, String>{
        if let Some(node) = self.find(path) {
            let item = node.read().unwrap();
            match item.deref() {
                FSItem::Directory(dir) => {
                    if self.side_effects {
                        let real_path = self.make_real_path(node.clone());
                        let target = Path::new(&real_path);

                        // check permissions from db
                        return Ok(0o755 as u32);
                    }

                    Ok(0o755 as u32)
                },
                FSItem::File(f) => {
                        if self.side_effects {
                            let real_path = self.make_real_path(node.clone());
                            let target = Path::new(&real_path);

                            // check permissions from db
                            return Ok(0o755 as u32);
                        }

                        Ok(0o755 as u32)
                },
                _ => Err("No file or directory in this path".to_string()),
            }
        }else{
            Err("Invalid path".to_string())
        }
    }

    #[cfg(target_family = "unix")]
    pub fn get_fs_stats(&self, path: &str) -> Result<Stats, String> {
        let path_object = Path::new(path);

        match statvfs(path_object){
            Ok(stats) => Ok(
                Stats {
                    blocks: stats.blocks(),
                    bfree: stats.blocks_free(),
                    bavail: stats.blocks_available(),
                    files: stats.files(),
                    ffree: stats.files_free(),
                    bsize: stats.block_size() as u32,
                    namelen: stats.name_max() as u32,
                    frsize: stats.fragment_size() as u32,
                }
            ),
            Err(e) => Err(format!("{:?}", e)),
        }
    }
}
