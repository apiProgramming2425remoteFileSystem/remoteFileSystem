use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs::{self, FileTimes, Permissions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

#[cfg(unix)]
use nix::sys::statvfs::statvfs;
#[cfg(unix)]
use nix::unistd::{Gid, Uid, chown};
use tracing::{Level, instrument};

use crate::attributes::{FileAttr, FileType, Operation, Permission, SetAttr, Stats, Timestamp};
use crate::error::StorageError;
use crate::nodes::{Directory, FSItem, File, SymLink};

type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug)]
pub struct FileSystem {
    real_path: PathBuf, // the real path of the file system
}

fn get_attributes_by_path<P: AsRef<Path> + Debug>(path: P) -> Result<FileAttr> {
    tracing::trace!("--PATH: {:?}", path);
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let mut nlink = 1;

            let kind = if metadata.is_dir() {
                nlink = 2;
                FileType::Directory
            } else if metadata.is_file() {
                FileType::RegularFile
            } else if metadata.is_symlink() {
                FileType::Symlink
            } else {
                FileType::RegularFile
            };

            let mut uid = metadata.uid();
            if uid < 1000 {
                uid += 1000;
            };

            let mut gid = metadata.gid();
            if gid < 1000 {
                gid += 1000;
            };

            let attributes = FileAttr {
                size: metadata.len(),
                blocks: 0, //  ? eventualmente modificare ?
                atime: Timestamp::from(metadata.accessed().unwrap()),
                mtime: Timestamp::from(metadata.modified().unwrap()),
                ctime: Timestamp::from(SystemTime::now()),
                crtime: Timestamp::from(SystemTime::now()),
                kind,
                perm: metadata.permissions().mode(),
                nlink,
                uid,
                gid,
                rdev: 0, // device ID of a special file in Unix-like operating systems, indicating the device associated with a file
                blksize: 0, // ? eventualmente modificare ?
                flags: 0, // macOS only
            };
            Ok(attributes)
        }
        Err(e) => Err(StorageError::NotFound(e.to_string())),
    }
}

fn set_owner(user_id: i64, group_id: i64, path: &PathBuf) -> Result<()> {
    let new_uid = Some(Uid::from_raw(user_id as u32));
    let new_gid = Some(Gid::from_raw(group_id as u32));
    chown(path, new_uid, new_gid).map_err(|e| StorageError::Other(e.into()))?;

    // We set group_permissions = user_permissions, if user_id = group_id
    if user_id == group_id {
        let item = fs::metadata(path).map_err(|e| StorageError::Other(e.into()))?;
        let original_perm = item.permissions().mode();

        let mut permission = Permission::try_from(original_perm as u16)
            .map_err(|_| StorageError::MetadataError("Error during convertion.".to_string()))?;
        permission.group = permission.user;
        let adjusted_perm: u16 = permission.into();

        let perms: Permissions = Permissions::from_mode(adjusted_perm as u32);
        fs::set_permissions(path, perms)?;
    }

    Ok(())
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

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub fn make_dir<P: AsRef<Path> + Debug, S: AsRef<OsStr> + Debug>(
        &self,
        user_id: i64,
        group_id: i64,
        path: P,
        name: S,
    ) -> Result<()> {
        let name = name.as_ref();
        let target = self.make_real_path(path)?.join(name);

        if target.exists() {
            return Err(StorageError::AlreadyExists(format!("{:?}", target)));
        }
        fs::create_dir(&target)?;
        set_owner(user_id, group_id, &target)?;

        return Ok(());
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
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

    #[instrument(skip(self), err(level = Level::ERROR))]
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

    #[instrument(skip(self), err(level = Level::ERROR))]
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

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub fn write_file<P: AsRef<Path> + Debug>(
        &self,
        user_id: i64,
        group_id: i64,
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
        set_owner(user_id, group_id, &real)?;

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

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn get_attributes<P: AsRef<Path> + Debug>(&self, path: P) -> Result<FileAttr> {
        let real_path = self.make_real_path(path)?;
        let target = Path::new(&real_path);

        return get_attributes_by_path(target);
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn get_permissions<P: AsRef<Path> + Debug>(&self, path: P) -> Result<u32> {
        let attributes = self.get_attributes(path)?;
        return Ok(attributes.perm);
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn set_attributes(
        &self,
        path: &str,
        user_id: i64,
        group_id: i64,
        new_attributes: SetAttr,
    ) -> Result<FileAttr> {
        let real_path = self.make_real_path(path)?;
        let file = fs::OpenOptions::new().write(true).open(&real_path)?;

        let times = FileTimes::new();
        let mut to_add = false;

        // REVIEW: refactor this function to make it cleaner

        // Allowed only if user is the owner or root
        if let Some(mode) = new_attributes.mode {
            if self.is_allowed(user_id, group_id, Path::new(path), Operation::OwnerOnly)? {
                let mut adjusted_mode = mode as u16;

                if user_id == group_id {
                    let mut permission = Permission::try_from(mode as u16).map_err(|_| {
                        StorageError::MetadataError("Error during convertion.".to_string())
                    })?;
                    permission.group = permission.user;
                    adjusted_mode = permission.into();
                }

                let perms = std::fs::Permissions::from_mode(adjusted_mode as u32);
                file.set_permissions(perms)?;
            } else {
                return Err(StorageError::PermissionDenied);
            }
        }

        if new_attributes.uid.is_some() {
            return Err(StorageError::PermissionDenied);
        }

        if let Some(client_gid) = new_attributes.gid {
            if self.is_allowed(user_id, group_id, Path::new(path), Operation::OwnerOnly)? {
                let new_uid = None;

                // We accept client_gid >= 1001, so when converted to server_gid there is no confusion with root_id = 0
                let mut server_gid = client_gid;
                if client_gid <= 1000 {
                    return Err(StorageError::PermissionDenied);
                } else {
                    server_gid -= 1000;
                }
                let new_gid = Some(Gid::from_raw(server_gid));

                chown(&real_path, new_uid, new_gid).map_err(|e| StorageError::Other(e.into()))?;
            } else {
                return Err(StorageError::PermissionDenied);
            }
        }

        // Allowed only if user has write permissions
        if let Some(size) = new_attributes.size {
            if self.is_allowed(user_id, group_id, Path::new(path), Operation::Write)? {
                file.set_len(size)?;
            } else {
                return Err(StorageError::PermissionDenied);
            }
        }
        if let Some(mtime) = new_attributes.mtime {
            if self.is_allowed(user_id, group_id, Path::new(path), Operation::Write)? {
                times.set_accessed(mtime.into());
                to_add = true;
            } else {
                return Err(StorageError::PermissionDenied);
            }
        }

        // Always allowed to set
        if let Some(atime) = new_attributes.atime {
            times.set_accessed(atime.into());
            to_add = true;
        }
        if to_add {
            file.set_times(times)?;
        }

        get_attributes_by_path(&real_path)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub fn is_allowed(
        &self,
        user_id: i64,
        group_id: i64,
        path: &Path,
        operation: Operation,
    ) -> Result<bool> {
        let mut path = self.make_real_path(path)?;
        if !path.exists() {
            path = PathBuf::from(path.parent().expect("Generic error"));
            // REVIEW: check if this is correct
            // return Err(StorageError::InvalidPath(format!(
            //     "Path {:?} does not exist",
            //     path
            // )));
        }

        let attributes = get_attributes_by_path(&path)?;
        let permissions = Permission::try_from(attributes.perm as u16)
            .map_err(|_| StorageError::MetadataError("Error during conversion.".to_string()))?;

        let owner_uid = (attributes.uid - 1000) as i64;
        let owner_gid = (attributes.gid - 1000) as i64;

        // if path owner is root, it means it has not been created by any user, so everyone can access to it
        if owner_uid == 0 {
            return Ok(true);
        }

        // 1. user_id == owner_uid -> check permissions
        if user_id == owner_uid {
            let user_permissions = permissions.user;
            match operation {
                Operation::Read => return Ok(user_permissions.read),
                Operation::Write => return Ok(user_permissions.write),
                Operation::Execute => return Ok(user_permissions.execute),
                Operation::OwnerOnly => return Ok(true),
            }
        } else if group_id == owner_gid {
            // 2. group_id == owner_gid -> check permissions
            let group_permissions = permissions.group;
            match operation {
                Operation::Read => return Ok(group_permissions.read),
                Operation::Write => return Ok(group_permissions.write),
                Operation::Execute => return Ok(group_permissions.execute),
                Operation::OwnerOnly => return Ok(false),
            }
        } else {
            // 3. check permissions for other
            let other_permissions = permissions.other;
            match operation {
                Operation::Read => return Ok(other_permissions.read),
                Operation::Write => return Ok(other_permissions.write),
                Operation::Execute => return Ok(other_permissions.execute),
                Operation::OwnerOnly => return Ok(false),
            }
        }
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
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
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
