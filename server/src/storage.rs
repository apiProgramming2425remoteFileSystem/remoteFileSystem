#[cfg(target_family = "unix")]
use nix::sys::statvfs::statvfs;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(target_family = "unix")]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use tracing::{Level, instrument};

use crate::error::StorageError;
use crate::models::{FileAttr, FileType, Permission, SetAttr, Stats, Timestamp};
use crate::nodes::{Directory, FSItem, File, SymLink};

type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug)]
pub struct FileSystem {
    real_path: PathBuf, // the real path of the file system
}

fn check_permission(owner_uid: u32, owner_gid: u32, uid: u32, gid: u32) -> bool {
    owner_uid == uid || uid == 0 || owner_gid == gid
}

fn get_attributes_by_path(path: &Path) -> Result<FileAttr> {
    match fs::symlink_metadata(path) {
        Ok(object) => {
            let nlink = 1;

            let kind = if object.is_dir() {
                FileType::Directory
            } else if object.is_file() {
                FileType::RegularFile
            } else if object.is_symlink() {
                FileType::Symlink
            } else {
                FileType::RegularFile
            };

            let attributes = FileAttr {
                size: object.len(),
                blocks: 0, //  ? eventualmente modificare ?
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
            return Err(StorageError::NotFound("There is no such node.".to_string()));
        }
    }
}

impl FileSystem {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<S: AsRef<OsStr> + Debug>(root: S) -> Self {
        let real_path = PathBuf::from(root.as_ref()).canonicalize().unwrap();
        FileSystem { real_path }
    }

    #[instrument(skip(self), ret(level = Level::TRACE))]
    fn make_real_path<P: AsRef<Path> + Debug>(&self, path: P) -> Result<PathBuf> {
        let clean: PathBuf = path
            .as_ref()
            .components()
            .filter(|c| *c != Component::RootDir)
            .collect();

        Ok(self.real_path.join(clean))
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

    //  taking only first grade children as we use only that...
    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find<P: AsRef<Path> + Debug>(&self, path: P) -> Option<FSItem> {
        let real = self.make_real_path(path.as_ref()).ok()?;
        let meta = real.symlink_metadata().ok()?;

        if meta.is_file() {
            let size = meta.len() as usize;
            let file = FSItem::File(File::new(
                real.file_name()?,
                get_attributes_by_path(&real).unwrap(),
            ));
            Some(file)
        } else if meta.is_dir() {
            let mut root = FSItem::Directory(Directory::new(
                real.file_name()?,
                get_attributes_by_path(&real).unwrap(),
            ));
            let entries = match fs::read_dir(&real) {
                Ok(entries) => entries,
                Err(err) => {
                    tracing::warn!("Cannot read directory {:?}: {}", real, err);
                    return None;
                }
            };

            for entry in entries {
                let entry = match entry {
                    Ok(e) => e,
                    Err(err) => {
                        tracing::warn!("Skipping unreadable entry: {}", err);
                        continue;
                    }
                };

                let path = entry.path();
                let meta = match std::fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(err) => {
                        tracing::warn!("Cannot read metadata for {:?}: {}", path, err);
                        continue;
                    }
                };

                let name = entry.file_name();

                let child = if meta.is_file() {
                    let path = real.join(name.clone());
                    FSItem::File(File::new(name, get_attributes_by_path(&path).unwrap()))
                } else if meta.is_dir() {
                    let path = real.join(name.clone());
                    FSItem::Directory(Directory::new(name, get_attributes_by_path(&path).unwrap()))
                } else if meta.is_symlink() {
                    let path = real.join(name.clone());
                    FSItem::SymLink(SymLink::new(name, get_attributes_by_path(&path).unwrap()))
                } else {
                    continue;
                };

                if let FSItem::Directory(dir) = &mut root {
                    dir.add(child);
                }
            }

            Some(root)
        } else if meta.is_symlink() {
            let link = FSItem::SymLink(SymLink::new(
                real.file_name()?,
                get_attributes_by_path(&real).unwrap(),
            ));
            Some(link)
        } else {
            None
        }
    }

    /* // taking all sub-tree recursively
    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn find_recursive<P: AsRef<Path> + Debug>(&self, path: P) -> Option<FSItem> {
        let real = self.make_real_path(path.as_ref()).ok()?;
        let meta = real.metadata().ok()?;

        if meta.is_file() {
            let size = meta.len() as usize;
            let file = FSItem::File(File::new(real.file_name()?, size));
            return Some(file);
        }

        let root_name = real.file_name().unwrap_or_else(|| OsStr::new(""));
        let mut root = FSItem::Directory(Directory::new(root_name));
        fn insert_path(root: &mut Directory, rel_path: &Path, item: FSItem) {
            let mut current = root;
            let mut components = rel_path.components().peekable();
            while let Some(c) = components.next() {
                let name = c.as_os_str();
                if components.peek().is_none() {
                    current.add(item);
                    return;
                }
                if let Some(child) = current.children.get_mut(&PathBuf::from(name)) {
                    match child {
                        FSItem::Directory(dir) => {
                            current = dir;
                        }
                        _ => {
                            tracing::warn!("Expected directory but found file at {:?}", rel_path);
                            return;
                        }
                    }
                } else {
                    let mut new_dir = Directory::new(name);
                    current.add(FSItem::Directory(new_dir.clone()));

                    current = current
                        .children
                        .get_mut(&PathBuf::from(name))
                        .unwrap()
                        .as_directory_mut()
                        .unwrap();
                }
            }
        }

        for entry in WalkDir::new(&real).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if entry.depth() == 0 {
                continue;
            }

            let rel_path = match path.strip_prefix(&real) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(err) => {
                    tracing::warn!("Skipping unreadable entry: {}", err);
                    continue;
                }
            };

            let name = entry.file_name();

            let item = if meta.is_file() {
                FSItem::File(File::new(name, meta.len() as usize))
            } else if meta.is_dir() {
                FSItem::Directory(Directory::new(name))
            } else if meta.is_symlink() {
                FSItem::SymLink(SymLink::new(name))
            } else {
                continue;
            }

            if let FSItem::Directory(dir) = &mut root {
                insert_path(dir, rel_path, item);
            }
        }

        Some(root)
    }

     */

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_dir<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &self,
        path: P,
        name: S,
    ) -> Result<()> {
        let name = name.as_ref();
        let target = self.make_real_path(path)?.join(name);
        if target.exists() {
            return Err(StorageError::AlreadyExists(format!("{:?}", target)));
        }
        fs::create_dir(&target)?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn make_file<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &self,
        path: P,
        name: S,
    ) -> Result<()> {
        let name = name.as_ref();
        let target = self.make_real_path(path)?.join(name);
        if target.exists() {
            return Err(StorageError::AlreadyExists(format!("{:?}", target)));
        }
        fs::File::create(&target)?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn rename<P: AsRef<Path> + Debug, S: AsRef<Path> + Debug>(
        &self,
        old_path: P,
        new_path: S,
    ) -> Result<()> {
        let real_old_path = self.make_real_path(old_path)?;
        let real_new_path = self.make_real_path(new_path)?;

        fs::rename(&real_old_path, &real_new_path)?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn delete<P: AsRef<Path> + Debug>(&self, path: P) -> Result<()> {
        let real = self.make_real_path(path.as_ref())?;
        let meta = real.symlink_metadata()?;

        if meta.file_type().is_symlink() {
            fs::remove_file(&real)?;
            Ok(())
        } else if meta.is_file() {
            fs::remove_file(&real)?;
            Ok(())
        } else if meta.is_dir() {
            fs::remove_dir_all(&real)?;
            Ok(())
        } else {
            Err(StorageError::NotFound(format!("{:?}", path)))
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn write_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        data: &[u8],
        offset: usize,
    ) -> Result<()> {
        let real = self.make_real_path(path)?;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&real)?;
        // Seek to offset
        f.seek(SeekFrom::Start(offset as u64))?;
        // Write data
        f.write_all(data)?;

        // useless??
        // Rewind to start to read the updated file content
        // f.seek(SeekFrom::Start(0))?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn read_file<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let real_path = self.make_real_path(path)?;
        let mut f = fs::OpenOptions::new().read(true).open(&real_path)?;
        f.seek(SeekFrom::Start(offset as u64))?;
        let mut buffer = vec![0u8; size];
        let bytes_read = f.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    pub fn get_attributes(&self, path: &str) -> Result<FileAttr> {
        let real_path = self.make_real_path(path)?;
        let target = Path::new(&real_path);
        get_attributes_by_path(target)
    }

    // Mirko poi vediamo insieme come crearla con il DB
    pub fn set_attributes(
        &self,
        path: &str,
        uid: u32,
        gid: u32,
        new_attributes: SetAttr,
    ) -> Result<FileAttr> {
        let real_path = self.make_real_path(path)?;
        if let Some(size) = new_attributes.size {
            // just doing touch
            let file = fs::OpenOptions::new().write(true).open(&real_path)?;
            file.set_len(size)?;
        }
        get_attributes_by_path(&real_path)
    }

    pub fn get_permissions(&self, path: &str) -> Result<u32> {
        let real = self.make_real_path(path)?;
        if !real.exists() {
            return Err(StorageError::InvalidPath(format!(
                "Path {:?} does not exist",
                path
            )));
        }
        Ok(0o755)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn read_symlink<P: AsRef<Path> + Debug>(&self, path: P) -> Result<String> {
        let real = self.make_real_path(path)?;
        let target = fs::read_link(&real)?;
        Ok(target.to_string_lossy().to_string())
    }

    #[cfg(target_family = "unix")]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn create_symlink<P: AsRef<Path> + Debug>(
        &self,
        path: P,
        target: &str,
    ) -> Result<FileAttr> {
        let real = self.make_real_path(path)?;

        if real.exists() {
            return Err(StorageError::AlreadyExists(format!("{:?}", real)));
        }

        symlink(target, &real)?;
        get_attributes_by_path(&real)
    }

    #[cfg(unix)]
    pub fn get_fs_stats(&self, path: &str) -> Result<Stats> {
        let real = self.make_real_path(path)?;
        if !real.exists() {
            return Err(StorageError::InvalidPath(format!(
                "Path {:?} does not exist",
                path
            )));
        }
        let path_object = Path::new(&real);

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
