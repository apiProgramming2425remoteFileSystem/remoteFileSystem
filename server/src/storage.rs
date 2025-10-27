use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::sync::Weak;

use tracing::{Level, instrument};
use walkdir::WalkDir;

use crate::error::StorageError;
use crate::nodes::{Directory, FSItem, FSNode, FSNodeWeak, File};

type Result<T> = std::result::Result<T, StorageError>;
/// Represents the in-memory file system structure
pub struct FileSystem {
    real_path: PathBuf, // the real path of the file system
    root: FSNode,
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
