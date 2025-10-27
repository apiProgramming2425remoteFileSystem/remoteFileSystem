use std::ffi::OsStr;
use std::fmt::{Debug, format};
use std::fs::{self, FileTimes, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Deref;
#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock, Weak};
use std::time::SystemTime;

use tracing::{Level, instrument};
use walkdir::WalkDir;

use crate::error::StorageError;
use crate::models::{Permission, SetAttr, Stats, Timestamp};
use crate::nodes::{Directory, FSItem, FSNode, FSNodeWeak, File};

use crate::models::{FileAttr, FileType};
#[cfg(target_family = "unix")]
use nix::sys::statvfs::{Statvfs, statvfs};

type Result<T> = std::result::Result<T, StorageError>;
/// Represents the in-memory file system structure
pub struct FileSystem {
    real_path: PathBuf, // the real path of the file system
    root: FSNode,
}

fn check_permission(owner_uid: u32, owner_gid: u32, uid: u32, gid: u32) -> bool {
    owner_uid == uid || uid == 0 || owner_gid == gid
}

fn get_attributes_by_path(path: &Path) -> Result<FileAttr> {
    match fs::metadata(path) {
        Ok(object) => {
            let nlink = 1;

            let kind = if object.is_dir() {
                FileType::Directory
            } else if object.is_file() {
                FileType::RegularFile
            } else {
                FileType::Symlink
            };

            let attributes = FileAttr {
                size: object.len(),
                blocks: 0, // ? eventualmente modificare ?
                atime: Timestamp::from(object.accessed().unwrap()),
                mtime: Timestamp::from(object.modified().unwrap()),
                ctime: Timestamp::from(SystemTime::now()),
                crtime: Timestamp::from(SystemTime::now()),
                kind: kind,
                perm: Permission::try_from(0o755 as u16).unwrap(),
                nlink: nlink,
                uid: 0,     // retrieve from db
                gid: 0,     // retrieve from db
                rdev: 0, // device ID of a special file in Unix-like operating systems, indicating the device associated with a file
                blksize: 0, // ? eventualmente modificare ?
                flags: 0, // macOS only
            };
            return Ok(attributes);
        }
        Err(_) => {
            return Err(StorageError::InvalidPath(
                "Error while obtaining file metadata.".to_string(),
            ));
        }
    }
}

impl FileSystem {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<S: AsRef<OsStr> + Debug>(root: S) -> Self {
        let real_path = PathBuf::from(root.as_ref()).canonicalize().unwrap();
        let root = FSNode::new(FSItem::Directory(Directory::new(&real_path, Weak::new())));

        FileSystem { real_path, root }
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub fn from_file_system<P: AsRef<Path> + Debug>(base_path: P) -> Result<Self> {
        let base = base_path.as_ref();
        let fs = FileSystem::new(base);

        let wdir = WalkDir::new(base)
            .min_depth(1)
            .into_iter()
            .filter_map(|res| {
                res.map_err(|err| {
                    tracing::warn!("skipping unreadable entry during walk: {}", err);
                    err
                })
                .ok()
            });

        for entry in wdir {
            // full fs path
            let entry_path = entry.path().to_path_buf();
            // remove base path, get relative path
            let Ok(rel_path) = entry_path.strip_prefix(&base).map_err(|err| {
                tracing::warn!(
                    "failed to strip prefix {:?} from {:?}: {:?}",
                    base,
                    entry_path,
                    err
                );
                err
            }) else {
                continue;
            };

            let mut current: FSNode = fs.root.clone();
            let next_node =
                Self::split_path(rel_path)
                    .into_iter()
                    .try_fold(fs.root.clone(), |cur, part| {
                        current = cur.clone();
                        cur.next(&part)
                    });

            // node already exists
            if next_node.is_some() {
                continue;
            }

            let name = entry.file_name();

            // child does not exist
            let new_node = if entry.file_type().is_file() {
                // create file node
                let size = entry.metadata().map(|m| m.len() as usize).unwrap_or(0);
                let file_item = FSItem::File(File::new(name, size, FSNodeWeak::from(&current)));
                FSNode::new(file_item)
            } else if entry.file_type().is_dir() {
                // create directory node
                let dir_item = FSItem::Directory(Directory::new(name, FSNodeWeak::from(&current)));
                FSNode::new(dir_item)
            } else {
                tracing::warn!("Skipping unsupported file type: {}", entry_path.display());
                continue;
            };

            current.write().add(new_node.clone());
        }

        Ok(fs)
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn make_real_path(&self, node: FSNode) -> Result<PathBuf> {
        let node = node.read();
        let mut abs_path = node.abs_path();

        abs_path = abs_path
            .components()
            .filter(|c| *c != Component::RootDir)
            .collect();

        Ok(PathBuf::from(&self.real_path)
            .join(&abs_path)
            .join(node.name()))
    }

    #[instrument(ret(level = Level::TRACE))]
    fn split_path<P: AsRef<Path> + Debug>(path: P) -> Vec<PathBuf> {
        let path = path.as_ref();
        path.components()
            .filter_map(|c| match c {
                Component::RootDir => None, // skip the RootDir
                _ => Some(PathBuf::from(c.as_os_str())),
            })
            .collect()
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find<P: AsRef<Path> + Debug>(&self, path: P) -> Option<FSNode> {
        self.find_full(path, None)
    }

    // find using either absolute or relative path
    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find_full<P: AsRef<Path> + Debug>(&self, path: P, base: Option<P>) -> Option<FSNode> {
        let path = path.as_ref();

        let current = if path.has_root() || base.is_none() {
            self.root.clone()
        } else {
            // if we can't find the base, return None
            self.find_full(base.unwrap(), None)?
        };

        Self::split_path(path)
            .into_iter()
            .try_fold(current, |cur, part| cur.next(&part))
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_dir<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &mut self,
        path: P,
        name: S,
    ) -> Result<()> {
        let Some(node) = self.find(path.as_ref()) else {
            return Err(StorageError::NotFound(format!("Directory {:?}", path)));
        };

        let name = name.as_ref();

        // create the directory on the file system
        let target = self.make_real_path(node.clone())?.join(name);
        fs::create_dir(&target)?;

        let new_dir = FSItem::Directory(Directory::new(name, FSNodeWeak::from(&node)));
        let new_node = FSNode::new(new_dir);
        node.write().add(new_node.clone());
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_file<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &mut self,
        path: P,
        name: S,
    ) -> Result<()> {
        let Some(node) = self.find(path.as_ref()) else {
            return Err(StorageError::NotFound(format!("Directory {:?}", path)));
        };

        let name = name.as_ref();
        let target = self.make_real_path(node.clone())?.join(name);

        if target.exists() {
            return Err(StorageError::AlreadyExists(format!("{:?}", target)));
        }
        fs::File::create(&target)?;

        let new_file = FSItem::File(File::new(name, 0, FSNodeWeak::from(&node)));
        let new_node = FSNode::new(new_file);
        node.write().add(new_node.clone());
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn rename<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &self,
        path: P,
        new_name: S,
    ) -> Result<()> {
        let Some(node) = self.find(path.as_ref()) else {
            return Err(StorageError::NotFound(format!("Item {:?}", path)));
        };

        let new_name = new_name.as_ref();
        let real_path = self.make_real_path(node.clone())?;
        let new_path = real_path.with_file_name(new_name);

        fs::rename(&real_path, &new_path)?;
        node.write().set_name(new_name);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn delete<P: AsRef<Path> + Debug>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        let Some(node) = self.find(path) else {
            return Err(StorageError::NotFound(format!("Item {:?}", path)));
        };
        let fs_item = node.read();

        match fs_item.deref() {
            FSItem::File(_) => {
                let real_path = self.make_real_path(node.clone())?;
                fs::remove_file(&real_path)?;
            }
            FSItem::Directory(_) => {
                let real_path = self.make_real_path(node.clone())?;
                fs::remove_dir_all(&real_path)?;
            }
        }

        let parent = FSNode::try_from(&fs_item.parent())?;

        parent.write().remove(&fs_item.name());
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn write_file<P: AsRef<Path> + Debug>(
        &mut self,
        path: P,
        data: &[u8],
        offset: usize,
    ) -> Result<()> {
        let path = path.as_ref();

        // Try to find node, or create file if not exists
        let node_opt = self.find(path).or_else(|| {
            let name = path.file_name()?;
            let dir = path.parent()?;

            // create file if not exists
            self.make_file(dir, name).ok()?;
            self.find(path)
        });

        let node = node_opt.ok_or_else(|| StorageError::NotFound("Path".into()))?;

        if node.is_directory() {
            return Err(StorageError::UnsupportedOperation(
                "Path is a directory, cannot write data".into(),
            ));
        }

        // write the file on the file system
        let real_path = self.make_real_path(node.clone())?;
        let file = node.write();
        let mut f = fs::OpenOptions::new().write(true).open(&real_path)?;
        // Seek to offset
        f.seek(SeekFrom::Start(offset as u64))?;
        // Write data
        f.write_all(data)?;
        // Rewind to start to read the updated file content
        f.seek(SeekFrom::Start(0))?;
        drop(file);
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn read_file<P: AsRef<Path> + Debug>(&self, path: P, offset: usize) -> Result<Vec<u8>> {
        let Some(node) = self.find(path.as_ref()) else {
            return Err(StorageError::NotFound(format!("Path {:?}", path)));
        };

        if node.is_directory() {
            return Err(StorageError::UnsupportedOperation(
                "Path is a directory, cannot read data.".into(),
            ));
        }

        // read the file from the real file system
        let real_path = self.make_real_path(node.clone())?;

        let mut f = fs::OpenOptions::new().read(true).open(&real_path)?;
        // Seek to offset
        f.seek(SeekFrom::Start(offset as u64))?;
        let mut buffer = Vec::<u8>::new();
        f.read_to_end(&mut buffer)?;

        Ok(buffer)
    }

    pub fn move_node<P: AsRef<Path> + Debug>(&self, old_path: P, new_path: P) -> Result<()> {
        // avoid moving a dir in its children (mv a/b a/b/c/d)
        if new_path.as_ref() == old_path.as_ref()
            || new_path.as_ref().starts_with(old_path.as_ref())
        {
            return Err(StorageError::InvalidPath("Old path has no parent".into()));
        }

        let old_parent_path = old_path
            .as_ref()
            .parent()
            .ok_or_else(|| StorageError::InvalidPath("Old path has no parent".into()))?;
        let old_name = old_path
            .as_ref()
            .file_name()
            .ok_or_else(|| StorageError::InvalidPath("Old path has no file name".into()))?;

        let parent_old = self
            .find(old_parent_path)
            .ok_or_else(|| StorageError::NotFound(format!("Old parent {:?}", old_parent_path)))?;

        let new_parent_path = new_path
            .as_ref()
            .parent()
            .ok_or_else(|| StorageError::InvalidPath("New path has no parent".into()))?;
        let new_name = new_path
            .as_ref()
            .file_name()
            .ok_or_else(|| StorageError::InvalidPath("New path has no file name".into()))?;

        if old_parent_path == new_parent_path {
            return self.rename(old_path, new_name);
        }

        let parent_new = self
            .find(new_parent_path)
            .ok_or_else(|| StorageError::NotFound(format!("New parent {:?}", new_parent_path)))?;

        let mut parent_old_guard = parent_old.write();

        let node_to_move = parent_old_guard
            .get_child(old_name)
            .ok_or_else(|| StorageError::NotFound(format!("Child {:?}", old_name)))?;

        parent_old_guard.remove(old_name);
        node_to_move.write().set_name(&new_name);

        let mut parent_new_guard = parent_new.write();
        parent_new_guard.add(node_to_move);

        Ok(())
    }

    /* IMPLEMENT PERMISSION MANAGEMENT VIA DB */
    pub fn get_attributes(&self, path: &str) -> Result<FileAttr> {
        if let Some(node) = self.find(path) {
            let real_path = self.make_real_path(node.clone())?;
            let target = Path::new(&real_path);

            return get_attributes_by_path(target);
        } else {
            Err(StorageError::InvalidPath(format!("{}", path)))
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
    pub fn set_attributes(
        &self,
        path: &str,
        uid: u32,
        gid: u32,
        new_attributes: SetAttr,
    ) -> Result<FileAttr> {
        if let Some(node) = self.find(path) {
            let real_path = self.make_real_path(node.clone())?;
            let target = Path::new(&real_path);

            match fs::metadata(target) {
                Ok(object) => {
                    let owner_uid = 0; // to substitute with call to db
                    let owner_gid = 0; // to substitute with call to db

                    /* ALWAYS ALLOWED CHANGES */
                    let new_times = FileTimes::new();

                    // access time
                    if new_attributes.atime.is_some() {
                        let new_atime = new_attributes.atime.unwrap();
                        new_times.set_accessed(SystemTime::from(new_atime));
                    }
                    // modification time
                    if new_attributes.mtime.is_some() {
                        let new_mtime = new_attributes.mtime.unwrap();
                        new_times.set_modified(SystemTime::from(new_mtime));
                    }
                    // creation time is automatically managed by kernel

                    /* CHANGES ALLOWED ONLY IF USER HAS PERMISSION */
                    let has_permission = check_permission(owner_uid, owner_gid, uid, gid);

                    if new_attributes.mode.is_some() {
                        if has_permission == false {
                            return Err(StorageError::PermissionDenied);
                        }
                        let new_mode = new_attributes.mode.unwrap();
                        // update info on db
                    }

                    if new_attributes.size.is_some() {
                        if has_permission == false {
                            return Err(StorageError::PermissionDenied);
                        }
                        let new_size = new_attributes.size.unwrap();
                        let file = OpenOptions::new().write(true).open(target)?;
                        file.set_len(new_size);
                    }

                    if new_attributes.uid.is_some() || new_attributes.gid.is_some() {
                        if has_permission == false {
                            return Err(StorageError::PermissionDenied);
                        }
                        let new_uid = if let Some(new) = new_attributes.uid {
                            new
                        } else {
                            uid
                        };

                        let new_gid = if let Some(new) = new_attributes.gid {
                            new
                        } else {
                            gid
                        };

                        // update db
                    }

                    // returns new attributes
                    let attributes = self.get_attributes(path).unwrap();
                    return Ok(attributes);
                }
                Err(_) => {
                    return Err(StorageError::MetadataError(
                        "Error while obtaining file metadata.".to_string(),
                    ));
                }
            }
        } else {
            Err(StorageError::InvalidPath(format!("{}", path)))
        }
    }

    /* IMPLEMENT PERMISSION MANAGEMENT VIA DB */
    pub fn get_permissions(&self, path: &str) -> Result<u32> {
        if let Some(node) = self.find(path) {
            let item = node.read();
            let real_path = self.make_real_path(node.clone())?;
            // let target = Path::new(&real_path);

            // check permissions from db
            Ok(0o755 as u32)
        } else {
            Err(StorageError::InvalidPath(format!("{}", path)))
        }
    }

    #[cfg(target_family = "unix")]
    pub fn get_fs_stats(&self, path: &str) -> Result<Stats> {
        let path_object = Path::new(path);

        match statvfs(path_object) {
            Ok(stats) => Ok(Stats {
                blocks: stats.blocks(),
                bfree: stats.blocks_free(),
                bavail: stats.blocks_available(),
                files: stats.files(),
                ffree: stats.files_free(),
                bsize: stats.block_size() as u32,
                namelen: stats.name_max() as u32,
                frsize: stats.fragment_size() as u32,
            }),
            Err(e) => Err(StorageError::MetadataError(format!("{:?}", e))),
        }
    }
}

impl Debug for FileSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSystem")
            .field("real_path", &self.real_path)
            .finish()?;

        writeln!(f, "\nRoot Directory:")?;
        draw_tree(f, &self.root, "")
    }
}

// Helper function to recursively write directory tree with branches
fn draw_tree(f: &mut std::fmt::Formatter<'_>, node: &FSNode, prefix: &str) -> std::fmt::Result {
    match node.read().deref() {
        FSItem::File(file) => writeln!(f, "{:?}", file),
        FSItem::Directory(dir) => {
            writeln!(f, "{:?}", dir)?;
            let len = dir.get_children().len();
            for (i, child) in dir.get_children().iter().enumerate() {
                let (new_prefix, branch) = if i + 1 == len {
                    (format!("{}    ", prefix), "└── ")
                } else {
                    (format!("{}│   ", prefix), "├── ")
                };
                write!(f, "{}{}", prefix, branch)?;
                draw_tree(f, child, &new_prefix)?;
            }
            Ok(())
        }
    }
}
