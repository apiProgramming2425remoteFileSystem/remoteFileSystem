use std::ffi::OsStr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::vec::IntoIter;

use crate::fs_model;
use crate::network::models::ItemType;

use bytes::Bytes;
use fuse3::path::prelude::*;
use fuse3::{Errno, Result};
use futures_util::stream;
use libc;
use tracing::{Level, instrument};

const TTL: Duration = Duration::from_secs(1);
const SEPARATOR: char = '/';

pub struct Fs {
    fs: fs_model::FileSystem,
}

impl Fs {
    pub fn new(base_url: &str) -> Self {
        Self {
            fs: fs_model::FileSystem::new(base_url),
        }
    }
}

/// pub async fn template_fn(&self, args) -> Result<> {
///     1. convert args to fs_model structures
///     2. call the needed self.fs function
///     3. converts the result
///     4. do other necessary operations
///     5. return the correct fuse3 result
/// }
//
impl PathFilesystem for Fs {
    /// dir entry stream given by [`readdir`][Filesystem::readdir].
    type DirEntryStream<'a>
        = stream::Iter<IntoIter<Result<DirectoryEntry>>>
    where
        Self: 'a;

    /// dir entry plus stream given by [`readdirplus`][Filesystem::readdirplus].
    type DirEntryPlusStream<'a>
        = stream::Iter<IntoIter<Result<DirectoryEntryPlus>>>
    where
        Self: 'a;

    /// initialize filesystem. Called before any other filesystem method.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn init(&self, req: Request) -> Result<ReplyInit> {
        tracing::info!("Filesystem initialized");
        /*
        Write buffer size...
        ogni chiamata write() scriverà al massimo quanto specificato qua
        (ma forse meno, in base a come gira al sistema operativo), per cui una sola operazione di scrittura
        potrebbe essere spezzata in tante write (per questo serve l'argomento offset).
        Quindi tenerne uno grande permette di ottenere un overhead minore perché verrà chiamata meno volte write()
        ma uno più piccolo potrebbe risultare in comunicazioni più agevoli visto che si tratta di un fs remoto
        e dobbiamo mandare robe in giro per la rete, in caso di pacchetti persi o simili immagino il recupero sia
        più veloce con uno spezzettamento più fine.
        Attualmente lascio un randomicissimo 64 KiB poi decidiamo insieme.
        */
        Ok(ReplyInit {
            max_write: NonZeroU32::new(64 * 1024).unwrap(),
        })
    }

    /// clean up filesystem. Called on filesystem exit which is fuseblk, in normal fuse filesystem,
    /// kernel may call forget for root. There is some discuss for this
    /// <https://github.com/bazil/fuse/issues/82#issuecomment-88126886>,
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    #[instrument(skip(self))]
    async fn destroy(&self, req: Request) -> () {
        tracing::info!("Filesystem destroy");
    }

    /// look up a directory entry by name and get its fs_model.
    #[instrument(skip(self), err(level = Level::DEBUG), ret(level = Level::DEBUG))]
    async fn lookup(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<ReplyEntry> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        if Path::new(format!("{:?}{:?}", parent, name).as_str()).exists() {
            let attr = if Path::new(name).is_dir() {
                self.fs.mock_dir_attr()
            } else {
                self.fs.mock_file_attr()
            };
            Ok(ReplyEntry {
                ttl: TTL,
                attr: attr.into(),
            })
        } else {
            Err(libc::ENOENT.into())
        }
    }

    /// forget an path. The nlookup parameter indicates the number of lookups previously
    /// performed on this path. If the filesystem implements path lifetimes, it is recommended
    /// that paths acquire a single reference on each lookup, and lose nlookup references on each
    /// forget. The filesystem may ignore forget calls, if the paths don\'t need to have a limited
    /// lifetime. On unmount it is not guaranteed, that all referenced paths will receive a forget
    /// message. When filesystem is normal(not fuseblk) and unmounting, kernel may send forget
    /// request for root and this library will stop session after call forget. There is some
    /// discussion for this <https://github.com/bazil/fuse/issues/82#issuecomment-88126886>,
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    #[instrument(skip(self))]
    async fn forget(&self, req: Request, parent: &OsStr, nlookup: u64) -> () {
        // TODO:
        tracing::warn!("[Not Implemented]");
    }

    /// get file fs_model. If `fh` is None, means `fh` is not set. If `path` is None, means the
    /// path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn getattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        flags: u32,
    ) -> Result<ReplyAttr> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        match path {
            Some(s) => {
                let attr = if Path::new(s).is_dir() {
                    self.fs.mock_dir_attr()
                } else {
                    self.fs.mock_file_attr()
                };
                Ok(ReplyAttr {
                    ttl: TTL,
                    attr: attr.into(),
                })
            }
            None => Err(libc::ENOSYS.into()),
        }
    }

    /// set file fs_model. If `fh` is None, means `fh` is not set. If `path` is None, means the
    /// path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn setattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        set_attr: SetAttr,
    ) -> Result<ReplyAttr> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())
        let attr = self.fs.mock_file_attr();
        Ok(ReplyAttr {
            ttl: TTL,
            attr: attr.into(),
        })
    }

    /// get an extended attribute. If size is too small, use [`ReplyXAttr::Size`] to return correct
    /// size. If size is enough, use [`ReplyXAttr::Data`] to send it, or return error.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn getxattr(
        &self,
        req: Request,
        path: &OsStr,
        name: &OsStr,
        size: u32,
    ) -> Result<ReplyXAttr> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// set an extended attribute.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn setxattr(
        &self,
        req: Request,
        path: &OsStr,
        name: &OsStr,
        value: &[u8],
        flags: u32,
        position: u32,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// list extended attribute names. If size is too small, use [`ReplyXAttr::Size`] to return
    /// correct size. If size is enough, use [`ReplyXAttr::Data`] to send it, or return error.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn listxattr(&self, req: Request, path: &OsStr, size: u32) -> Result<ReplyXAttr> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// remove an extended attribute.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn removexattr(&self, req: Request, path: &OsStr, name: &OsStr) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// get filesystem statistics.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn statfs(&self, req: Request, path: &OsStr) -> Result<ReplyStatFs> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// check file access permissions. This will be called for the `access()` system call. If the
    /// `default_permissions` mount option is given, this method is not be called. This method is
    /// not called under Linux kernel versions 2.4.x.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn access(&self, req: Request, path: &OsStr, mask: u32) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// map block index within file to block index within device.
    ///
    /// # Notes:
    ///
    /// This may not works because currently this crate doesn\'t support fuseblk mode yet.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn bmap(
        &self,
        req: Request,
        path: &OsStr,
        block_size: u32,
        idx: u64,
    ) -> Result<ReplyBmap> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// find next data or hole after the specified offset.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn lseek(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        whence: u32,
    ) -> Result<ReplyLSeek> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// create file node. Create a regular file, character device, block device, fifo or socket
    /// node. When creating file, most cases user only need to implement
    /// [`create`][PathFilesystem::create].
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn mknod(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        rdev: u32,
    ) -> Result<ReplyEntry> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let Ok(fs_type) = fs_model::FileType::try_from(mode) else {
            return Err(libc::EINVAL.into());
        };

        let file_attr = self
            .fs
            .create_file(
                req.uid,
                req.gid,
                &PathBuf::from(parent),
                &PathBuf::from(name),
                &fs_type,
            )
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyEntry {
            ttl: TTL,
            attr: file_attr.into(),
        })
    }

    /// create and open a file. If the file does not exist, first create it with the specified
    /// mode, and then open it. Open flags (with the exception of `O_NOCTTY`) are available in
    /// flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in `fh`, and use
    /// this in other all other file operations ([`read`][PathFilesystem::read],
    /// [`write`][PathFilesystem::write], [`flush`][PathFilesystem::flush],
    /// [`release`][PathFilesystem::release], [`fsync`][PathFilesystem::fsync]). There are also
    /// some flags (`direct_io`, `keep_cache`) which the filesystem may set, to change the way the
    /// file is opened. If this method is not implemented or under Linux kernel versions earlier
    /// than 2.6.15, the [`mknod`][PathFilesystem::mknod] and [`open`][PathFilesystem::open]
    /// methods will be called instead.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn create(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        flags: u32,
    ) -> Result<ReplyCreated> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let parent_path = PathBuf::from(parent);
        let name_path = PathBuf::from(name);

        let Ok(fs_type) = fs_model::FileType::try_from(mode) else {
            return Err(libc::EINVAL.into());
        };
        let Ok(fs_flags) = fs_model::Flags::try_from(flags) else {
            return Err(libc::EINVAL.into());
        };

        if fs_flags.noctt {
            return Err(libc::EINVAL.into());
        }

        let file_attr = self
            .fs
            .create_file(req.uid, req.gid, &parent_path, &name_path, &fs_type)
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        let fh = self
            .fs
            .open_file(req.uid, req.gid, &parent_path.join(name_path), &fs_flags)
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyCreated {
            ttl: TTL,
            attr: file_attr.into(),
            generation: 0,
            fh,
            flags,
        })
    }

    /// open a file. Open flags (with the exception of `O_CREAT`, `O_EXCL` and `O_NOCTTY`) are
    /// available in flags. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// fh, and use this in other all other file operations (read, write, flush, release, fsync).
    /// Filesystem may also implement stateless file I/O and not store anything in fh. There are
    /// also some flags (`direct_io`, `keep_cache`) which the filesystem may set, to change the way
    /// the file is opened.  A file system need not implement this method if it
    /// sets [`MountOptions::no_open_support`][crate::MountOptions::no_open_support] and if the
    /// kernel supports `FUSE_NO_OPEN_SUPPORT`.
    ///
    /// # Notes:
    ///
    /// See `fuse_file_info` structure in
    /// [fuse_common.h](https://libfuse.github.io/doxygen/include_2fuse__common_8h_source.html) for
    /// more details.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn open(&self, req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let Ok(fs_flags) = fs_model::Flags::try_from(flags) else {
            return Err(libc::EINVAL.into());
        };

        if fs_flags.create || fs_flags.excl || fs_flags.noctt {
            return Err(libc::EINVAL.into());
        }

        let fh = self
            .fs
            .open_file(req.uid, req.gid, &PathBuf::from(path), &fs_flags)
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyOpen { fh, flags })
    }

    /// read data. Read should send exactly the number of bytes requested except on EOF or error,
    /// otherwise the rest of the data will be substituted with zeroes. An exception to this is
    /// when the file has been opened in `direct_io` mode, in which case the return value of the
    /// read system call will reflect the return value of this operation. `fh` will contain the
    /// value set by the open method, or will be undefined if the open method didn\'t set any value.
    /// when `path` is None, it means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn read(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> Result<ReplyData> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            return Err(libc::EINVAL.into());
        };

        let data = self
            .fs
            .read_file(req.uid, req.gid, &file_path, offset as usize, size as usize)
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyData { data: data.into() })
    }

    /// write data. Write should return exactly the number of bytes requested except on error. An
    /// exception to this is when the file has been opened in `direct_io` mode, in which case the
    /// return value of the write system call will reflect the return value of this operation. `fh`
    /// will contain the value set by the open method, or will be undefined if the open method
    /// didn\'t set any value. When `path` is None, it means the path may be deleted. When
    /// `write_flags` contains [`FUSE_WRITE_CACHE`](crate::raw::flags::FUSE_WRITE_CACHE), means the
    /// write operation is a delay write.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn write(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        data: &[u8],
        write_flags: u32,
        flags: u32,
    ) -> Result<ReplyWrite> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            return Err(libc::EINVAL.into());
        };

        let Ok(fs_flags) = fs_model::Flags::try_from(flags) else {
            return Err(libc::EINVAL.into());
        };

        let write_data = self
            .fs
            .write_file(
                req.uid,
                req.gid,
                &file_path,
                &fs_flags,
                offset as usize,
                data,
            )
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyWrite {
            written: write_data as u32,
        })
    }

    /// flush method. This is called on each `close()` of the opened file. Since file descriptors
    /// can be duplicated (`dup`, `dup2`, `fork`), for one open call there may be many flush calls.
    /// Filesystems shouldn\'t assume that flush will always be called after some writes, or that if
    /// will be called at all. `fh` will contain the value set by the open method, or will be
    /// undefined if the open method didn\'t set any value. when `path` is None, it means the path
    /// may be deleted.
    ///
    /// # Notes:
    ///
    /// the name of the method is misleading, since (unlike fsync) the filesystem is not forced to
    /// flush pending writes. One reason to flush data, is if the filesystem wants to return write
    /// errors. If the filesystem supports file locking operations (
    /// [`setlk`][PathFilesystem::setlk], [`getlk`][PathFilesystem::getlk]) it should remove all
    /// locks belonging to `lock_owner`.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn flush(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// release an open file. Release is called when there are no more references to an open file:
    /// all file descriptors are closed and all memory mappings are unmapped. For every open call
    /// there will be exactly one release call. The filesystem may reply with an error, but error
    /// values are not returned to `close()` or `munmap()` which triggered the release. `fh` will
    /// contain the value set by the open method, or will be undefined if the open method didn\'t
    /// set any value. `flags` will contain the same flags as for open. `flush` means flush the
    /// data or not when closing file. when `path` is None, it means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn release(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// synchronize file contents. If the `datasync` is true, then only the user data should be
    /// flushed, not the metadata. when `path` is None, it means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn fsync(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        datasync: bool,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// copy a range of data from one file to another. This can improve performance because it
    /// reduce data copy: in normal, data will copy from FUSE server to kernel, then to user-space,
    /// then to kernel, finally send back to FUSE server. By implement this method, data will only
    /// copy in FUSE server internal.  when `from_path` or `to_path` is None, it means the path may
    /// be deleted.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn copy_file_range(
        &self,
        req: Request,
        from_path: Option<&OsStr>,
        fh_in: u64,
        offset_in: u64,
        to_path: Option<&OsStr>,
        fh_out: u64,
        offset_out: u64,
        length: u64,
        flags: u64,
    ) -> Result<ReplyCopyFileRange> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let file_in_path = if let Some(p) = from_path {
            PathBuf::from(p)
        } else {
            return Err(libc::EINVAL.into());
        };

        let file_out_path = if let Some(p) = to_path {
            PathBuf::from(p)
        } else {
            return Err(libc::EINVAL.into());
        };

        let data = self
            .fs
            .read_file(
                req.uid,
                req.gid,
                &file_in_path,
                offset_in as usize,
                length as usize,
            )
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        let Ok(fs_flags) = fs_model::Flags::try_from(flags) else {
            return Err(libc::EINVAL.into());
        };

        let write_data = self
            .fs
            .write_file(
                req.uid,
                req.gid,
                &file_out_path,
                &fs_flags,
                length as usize,
                data.as_slice(),
            )
            .await
            .map_err(|err| {
                tracing::error!("{err}");
                libc::ENOSYS
            })?;

        Ok(ReplyCopyFileRange {
            copied: write_data as u64,
        })
    }

    /// allocate space for an open file. This function ensures that required space is allocated for
    /// specified file.
    ///
    /// # Notes:
    ///
    /// more information about `fallocate`, please see **`man 2 fallocate`**
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn fallocate(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// create a directory.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn mkdir(
        &self,
        _req: Request,
        parent: &OsStr,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
    ) -> Result<ReplyEntry> {
        let parent_path = Path::new(parent);
        let complete_path = parent_path.join(name);
        match self.fs.mkdir(complete_path.as_os_str()).await {
            Ok(()) => (),
            Err(err) => {
                tracing::error!("mkdir failed: {err}");
                return Err(Errno::from(libc::EIO)); //  generic I/O error
            }
        };
        let attr = if Path::new(name).is_dir() {
            self.fs.mock_dir_attr()
        } else {
            self.fs.mock_file_attr()
        };
        Ok(ReplyEntry {
            ttl: TTL,
            attr: attr.into(),
        })
    }

    /// remove a directory.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn rmdir(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// open a directory. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// `fh`, and use this in other all other directory stream operations
    /// ([`readdir`][PathFilesystem::readdir], [`releasedir`][PathFilesystem::releasedir],
    /// [`fsyncdir`][PathFilesystem::fsyncdir]). Filesystem may also implement stateless directory
    /// I/O and not store anything in `fh`.  A file system need not implement this method if it
    /// sets [`MountOptions::no_open_dir_support`][crate::MountOptions::no_open_dir_support] and if
    /// the kernel supports `FUSE_NO_OPENDIR_SUPPORT`.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn opendir(&self, req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        Ok(ReplyOpen { fh: 1, flags: 0 })
    }

    /// read directory. `offset` is used to track the offset of the directory entries. `fh` will
    /// contain the value set by the [`opendir`][PathFilesystem::opendir] method, or will be
    /// undefined if the [`opendir`][PathFilesystem::opendir] method didn\'t set any value.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn readdir<'a>(
        &'a self,
        req: Request,
        path: &'a OsStr,
        fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory<Self::DirEntryStream<'a>>> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let mut entries: Vec<Result<DirectoryEntry>> = Vec::new();

        if offset == 0 {
            entries.push(Ok(DirectoryEntry {
                offset: 1,
                name: OsStr::new(".").into(),
                kind: FileType::Directory,
            }));
        }
        if offset <= 1 {
            entries.push(Ok(DirectoryEntry {
                offset: 2,
                name: OsStr::new("..").into(),
                kind: FileType::Directory,
            }));
        }

        let items = match self.fs.list_path(path).await {
            Ok(vec_items) => vec_items,
            Err(err) => {
                tracing::error!("list_path failed: {err}");
                return Err(Errno::from(libc::EIO)); //  generic I/O error
            }
        };

        let other_entries: Vec<Result<DirectoryEntry>> = items
            .into_iter()
            .skip(offset.saturating_sub(2) as usize)
            .enumerate()
            .map(|(idx, item)| {
                let kind = match item.item_type {
                    ItemType::File => FileType::RegularFile,
                    ItemType::Directory => FileType::Directory,
                };
                Ok(DirectoryEntry {
                    offset: (offset + idx as i64 + 3),
                    name: item.name.into(),
                    kind,
                })
            })
            .collect();

        entries.extend(other_entries);

        let stream = stream::iter(entries);
        Ok(ReplyDirectory { entries: stream })
    }

    /// read directory entries, but with their attribute, like [`readdir`][PathFilesystem::readdir]
    /// + [`lookup`][PathFilesystem::lookup] at the same time.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn readdirplus<'a>(
        &'a self,
        req: Request,
        parent: &'a OsStr,
        fh: u64,
        offset: u64,
        lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        let mut entries: Vec<Result<DirectoryEntryPlus>> = Vec::new();

        if offset == 0 {
            entries.push(Ok(DirectoryEntryPlus {
                kind: FileType::Directory,
                name: OsStr::new(".").into(),
                offset: 1,
                attr: self.fs.mock_dir_attr().into(),
                entry_ttl: TTL,
                attr_ttl: TTL,
            }));
        }
        if offset <= 1 {
            entries.push(Ok(DirectoryEntryPlus {
                kind: FileType::Directory,
                name: OsStr::new("..").into(),
                offset: 2,
                attr: self.fs.mock_dir_attr().into(),
                entry_ttl: TTL,
                attr_ttl: TTL,
            }));
        }

        let items = match self.fs.list_path(parent).await {
            Ok(vec_items) => vec_items,
            Err(err) => {
                tracing::error!("list_path failed: {err}");
                return Err(Errno::from(libc::EIO));
            }
        };

        let other_entries: Vec<Result<DirectoryEntryPlus>> = items
            .into_iter()
            .skip(offset.saturating_sub(2) as usize)
            .enumerate()
            .map(|(idx, item)| {
                let (kind, attr) = match item.item_type {
                    ItemType::File => (FileType::RegularFile, self.fs.mock_file_attr()),
                    ItemType::Directory => (FileType::Directory, self.fs.mock_dir_attr()),
                };
                Ok(DirectoryEntryPlus {
                    kind,
                    name: item.name.into(),
                    offset: (offset + idx as u64 + 3) as i64,
                    attr: attr.into(),
                    entry_ttl: TTL,
                    attr_ttl: TTL,
                })
            })
            .collect();

        entries.extend(other_entries);

        let stream = stream::iter(entries);
        Ok(ReplyDirectoryPlus { entries: stream })
    }

    /// release an open directory. For every [`opendir`][PathFilesystem::opendir] call there will
    /// be exactly one `releasedir` call. `fh` will contain the value set by the
    /// [`opendir`][PathFilesystem::opendir] method, or will be undefined if the
    /// [`opendir`][PathFilesystem::opendir] method didn\'t set any value.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn releasedir(&self, req: Request, path: &OsStr, fh: u64, flags: u32) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())

        Ok(())
    }

    /// synchronize directory contents. If the `datasync` is true, then only the directory contents
    /// should be flushed, not the metadata. `fh` will contain the value set by the
    /// [`opendir`][PathFilesystem::opendir] method, or will be undefined if the
    /// [`opendir`][PathFilesystem::opendir] method didn\'t set any value.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn fsyncdir(&self, req: Request, path: &OsStr, fh: u64, datasync: bool) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// rename a file or directory.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn rename(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
    ) -> Result<()> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())
        let old_path = Path::new(origin_parent).join(origin_name);
        let new_path = Path::new(parent).join(name);
        match self
            .fs
            .rename(old_path.as_os_str(), new_path.as_os_str())
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(Errno::from(libc::ENOENT)),
        }
    }

    /// rename a file or directory with flags.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn rename2(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
        flags: u32,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// create a hard link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn link(
        &self,
        req: Request,
        path: &OsStr,
        new_parent: &OsStr,
        new_name: &OsStr,
    ) -> Result<ReplyEntry> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// remove a file.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn unlink(&self, req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// read symbolic link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn readlink(&self, req: Request, path: &OsStr) -> Result<ReplyData> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// create a symbolic link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn symlink(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        link_path: &OsStr,
    ) -> Result<ReplyEntry> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// test for a POSIX file lock.
    ///
    /// # Notes:
    ///
    /// this is supported on enable **`file-lock`** feature.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn getlk(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
    ) -> Result<ReplyLock> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// acquire, modify or release a POSIX file lock."]
    ///
    /// # Notes:
    ///
    /// this is supported on enable **`file-lock`** feature.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn setlk(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        r#type: u32,
        pid: u32,
        block: bool,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// handle interrupt. When a operation is interrupted, an interrupt request will send to fuse
    /// server with the unique id of the operation.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn interrupt(&self, req: Request, unique: u64) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// poll for IO readiness events.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn poll(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        kn: Option<u64>,
        flags: u32,
        envents: u32,
        notify: &Notify,
    ) -> Result<ReplyPoll> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// receive notify reply from kernel.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn notify_reply(
        &self,
        req: Request,
        path: &OsStr,
        offset: u64,
        data: Bytes,
    ) -> Result<()> {
        // TODO:
        tracing::warn!("[Not Implemented]");
        Err(libc::ENOSYS.into())
    }

    /// forget more than one path. This is a batch version [`forget`][PathFilesystem::forget]
    #[instrument(skip(self))]
    async fn batch_forget(&self, req: Request, paths: &[&OsStr]) -> () {
        tracing::debug!("")
    }
}

impl From<fs_model::FileAttr> for FileAttr {
    fn from(value: fs_model::FileAttr) -> Self {
        Self {
            size: value.size,
            blocks: value.blocks,
            atime: value.atime,
            mtime: value.mtime,
            ctime: value.ctime,
            kind: value.kind.into(),
            perm: value.perm.into(),
            nlink: value.nlink,
            uid: value.uid,
            gid: value.gid,
            rdev: value.rdev,
            blksize: value.blksize,
        }
    }
}

impl From<fs_model::FileType> for FileType {
    fn from(value: fs_model::FileType) -> Self {
        match value {
            fs_model::FileType::NamedPipe => Self::NamedPipe,
            fs_model::FileType::CharDevice => Self::CharDevice,
            fs_model::FileType::BlockDevice => Self::BlockDevice,
            fs_model::FileType::Directory => Self::Directory,
            fs_model::FileType::RegularFile => Self::RegularFile,
            fs_model::FileType::Symlink => Self::Symlink,
            fs_model::FileType::Socket => Self::Socket,
        }
    }
}
