use std::ffi::OsStr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::vec::IntoIter;

use super::*;
use crate::error::{FsModelError, FuseError, NetworkError};
use crate::fs_model;
use crate::network::models::ItemType;

use bytes::Bytes;
use fuse3::path::prelude::*;
use fuse3::{Errno, Result as FuseResult};
use futures_util::stream;
use libc;
use tracing::{Level, instrument};

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
        = stream::Iter<IntoIter<FuseResult<DirectoryEntry>>>
    where
        Self: 'a;

    /// dir entry plus stream given by [`readdirplus`][Filesystem::readdirplus].
    type DirEntryPlusStream<'a>
        = stream::Iter<IntoIter<FuseResult<DirectoryEntryPlus>>>
    where
        Self: 'a;

    /// initialize filesystem. Called before any other filesystem method.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn init(&self, req: Request) -> FuseResult<ReplyInit> {
        tracing::info!("Filesystem initialized");
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
        tracing::info!("Destroying file system...");
    }

    /****************
    Presente inserimento del token di autenticazione
    ****************/
    /// look up a directory entry by name and get its attributes.
    #[instrument(skip(self), err(level = Level::DEBUG), ret(level = Level::DEBUG))]
    async fn lookup(&self, req: Request, parent: &OsStr, name: &OsStr) -> FuseResult<ReplyEntry> {
        /**********************************************
         * Di fatto agisce esattamente come la getattr...
         ***********************************************/

        let path = PathBuf::from(parent).join(name);

        let attributes = self.fs.get_attributes(&path).await?;

        Ok(ReplyEntry {
            ttl: self.fs.get_ttl(),
            attr: attributes.into(),
        })
    }

    /// forget an path. The nlookup parameter indicates the number of lookups previously
    /// performed on this path. If the filesystem implements path lifetimes, it is recommended
    /// that paths acquire a single reference on each lookup, and lose nlookup references on each
    /// forget. The filesystem may ignore forget calls, if the paths don't need to have a limited
    /// lifetime. On unmount it is not guaranteed, that all referenced paths will receive a forget
    /// message. When filesystem is normal(not fuseblk) and unmounting, kernel may send forget
    /// request for root and this library will stop session after call forget. There is some
    /// discussion for this <https://github.com/bazil/fuse/issues/82#issuecomment-88126886>,
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    /// <https://sourceforge.net/p/fuse/mailman/message/31995737/>
    #[instrument(skip(self))]
    async fn forget(&self, req: Request, parent: &OsStr, nlookup: u64) -> () {
        let path = Path::new(parent);
        self.fs.cache_invalidate(path);
    }

    /// get file fs_model.
    /// If `fh` is None, means `fh` is not set.
    /// If `path` is None, means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn getattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        flags: u32,
    ) -> FuseResult<ReplyAttr> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Some(fh) = fh else {
                return Err(FuseError::NotFound("File handle".to_string()).into());
            };
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidFileHandle(fh).into());
            };
            p
        };

        let attributes = self.fs.get_attributes(&path).await?;

        Ok(ReplyAttr {
            ttl: self.fs.get_ttl(),
            attr: attributes.into(),
        })
    }

    /// set file fs_model. If `fh` is None, means `fh` is not set. If `path` is None, means the
    /// path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn setattr(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: Option<u64>,
        set_attr: fuse3::SetAttr,
    ) -> FuseResult<ReplyAttr> {
        let path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            return Err(libc::EINVAL.into());
        };

        let attributes = self
            .fs
            .set_attributes(req.uid, req.gid, &path, set_attr.into())
            .await?;

        Ok(ReplyAttr {
            ttl: self.fs.get_ttl(),
            attr: attributes.into(),
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
    ) -> FuseResult<ReplyXAttr> {
        if self.fs.use_xattributes() {
            let path = Path::new(path);
            let name = name.to_str().ok_or_else(|| {
                FuseError::InvalidInput("Attributes name is not valid UTF-8".to_string())
            })?;
            let xattr = self.fs.get_x_attributes(path, name).await?;
            if size == 0 {
                return Ok(ReplyXAttr::Size(xattr.len() as u32));
            }
            if size < xattr.len() as u32 {
                return Err(libc::ERANGE.into());
            }
            Ok(ReplyXAttr::Data(Bytes::from(xattr)))
        } else {
            Err(FuseError::Unsupported("getxattr".to_string()).into())
        }
    }

    /// set an extended attribute.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn setxattr(
        &self,
        req: Request,
        path: &OsStr,
        name: &OsStr,
        value: &[u8],
        flags: u32,
        position: u32,
    ) -> FuseResult<()> {
        if self.fs.use_xattributes() {
            let path = Path::new(path);
            let name = name.to_str().ok_or_else(|| {
                FuseError::InvalidInput("Attributes name is not valid UTF-8".to_string())
            })?;
            self.fs
                .set_x_attributes(path, name, value, flags, position)
                .await?;
            Ok(())
        } else {
            Err(FuseError::Unsupported("setxattr".to_string()).into())
        }
    }

    /// list extended attribute names. If size is too small, use [`ReplyXAttr::Size`] to return
    /// correct size. If size is enough, use [`ReplyXAttr::Data`] to send it, or return error.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn listxattr(&self, req: Request, path: &OsStr, size: u32) -> FuseResult<ReplyXAttr> {
        if self.fs.use_xattributes() {
            let path = Path::new(path);
            let names = self.fs.list_x_attribute(path).await?;
            let mut buf = Vec::new();
            for name in &names {
                buf.extend_from_slice(name.as_bytes());
                buf.push(0);
            }
            if size == 0 {
                return Ok(ReplyXAttr::Size(buf.len() as u32));
            }
            if size < buf.len() as u32 {
                return Err(libc::ERANGE.into());
            }
            Ok(ReplyXAttr::Data(Bytes::from(buf)))
        } else {
            Err(FuseError::Unsupported("listxattr".to_string()).into())
        }
    }

    /// remove an extended attribute.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn removexattr(&self, req: Request, path: &OsStr, name: &OsStr) -> FuseResult<()> {
        if self.fs.use_xattributes() {
            let path = Path::new(path);
            let name = name.to_str().ok_or_else(|| {
                FuseError::InvalidInput("Attributes name is not valid UTF-8".to_string())
            })?;
            self.fs.remove_x_attributes(path, name).await?;
            Ok(())
        } else {
            Err(FuseError::Unsupported("removexattr".to_string()).into())
        }
    }

    /// get filesystem statistics.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn statfs(&self, req: Request, path: &OsStr) -> FuseResult<ReplyStatFs> {
        // TODO:
        //Err(FuseError::NotImplemented.into())

        let path = PathBuf::from(path);
        let stats = self.fs.get_fs_stats(&path).await?;

        Ok(ReplyStatFs {
            blocks: stats.blocks,
            bfree: stats.bfree,
            bavail: stats.bavail,
            bsize: stats.bsize,
            frsize: stats.frsize,
            files: stats.files,
            ffree: stats.ffree,
            namelen: stats.namelen,
        })
    }

    /// check file access permissions. This will be called for the `access()` system call. If the
    /// `default_permissions` mount option is given, this method is not be called. This method is
    /// not called under Linux kernel versions 2.4.x.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn access(&self, req: Request, path: &OsStr, mask: u32) -> FuseResult<()> {
        let path = PathBuf::from(path);

        self.fs.get_permissions(&path, mask).await?;

        Ok(())
    }

    /// map block index within file to block index within device.
    ///
    /// # Notes:
    ///
    /// This may not works because currently this crate doesn't support fuseblk mode yet.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn bmap(
        &self,
        req: Request,
        path: &OsStr,
        block_size: u32,
        idx: u64,
    ) -> FuseResult<ReplyBmap> {
        // TODO:
        Err(FuseError::Unsupported("bmap".to_string()).into())
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
    ) -> FuseResult<ReplyLSeek> {
        // TODO:
        Err(FuseError::Unsupported("lseek".to_string()).into())
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
    ) -> FuseResult<ReplyEntry> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let path = PathBuf::from(parent).join(name);

        let fs_type = fs_model::FileType::try_from(mode)?;

        let file_attr = self.fs.create_file(&path, &fs_type, 0, &[]).await?;

        Ok(ReplyEntry {
            ttl: self.fs.get_ttl(),
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
    ) -> FuseResult<ReplyCreated> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let path = PathBuf::from(parent).join(name);

        let fs_type = fs_model::FileType::try_from(mode)?;
        let fs_flags = fs_model::Flags::try_from(flags)?;

        if fs_flags.noctt {
            return Err(
                FuseError::InvalidInput("`O_NOCTTY` flag not supported".to_string()).into(),
            );
        }

        let file_attr = self.fs.create_file(&path, &fs_type, 0, &[]).await?;

        let fh = self.fs.open(&path, &fs_flags).await?;

        Ok(ReplyCreated {
            ttl: self.fs.get_ttl(),
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
    async fn open(&self, req: Request, path: &OsStr, flags: u32) -> FuseResult<ReplyOpen> {
        // Err(FuseError::NotImplemented.into())
        let path = Path::new(path);

        let fs_flags = fs_model::Flags::try_from(flags)?;

        if fs_flags.create || fs_flags.excl || fs_flags.noctt {
            return Err(FuseError::InvalidInput("Invalid open flags".to_string()).into());
        }

        let fh = self.fs.open(path, &fs_flags).await?;

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
    ) -> FuseResult<ReplyData> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidInput("Invalid file handle".to_string()).into());
            };
            p
        };

        let data = self
            .fs
            .read_file(
                &file_path,
                offset as usize,
                size as usize,
                // &self.auth_token,
            )
            .await?;

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
    ) -> FuseResult<ReplyWrite> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidInput("Invalid file handle".to_string()).into());
            };
            p
        };

        let fs_flags = fs_model::Flags::try_from(flags)?;

        let write_data = self
            .fs
            .write_file(
                &file_path,
                &fs_flags,
                offset as usize,
                data,
                // &self.auth_token,
            )
            .await?;

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
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn flush(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        lock_owner: u64,
    ) -> FuseResult<()> {
        self.fs.flush_write_buffer().await?;
        Ok(())
    }

    /// release an open file. Release is called when there are no more references to an open file:
    /// all file descriptors are closed and all memory mappings are unmapped. For every open call
    /// there will be exactly one release call. The filesystem may reply with an error, but error
    /// values are not returned to `close()` or `munmap()` which triggered the release. `fh` will
    /// contain the value set by the open method, or will be undefined if the open method didn\'t
    /// set any value. `flags` will contain the same flags as for open. `flush` means flush the
    /// data or not when closing file. when `path` is None, it means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn release(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> FuseResult<()> {
        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidFileHandle(fh).into());
            };
            p
        };

        let fs_flags = fs_model::Flags::try_from(flags)?;

        self.fs.release(&file_path, &fs_flags, fh).await?;

        if flush {
            self.fs.flush_write_buffer().await?;
        }

        Ok(())
    }

    /// synchronize file contents. If the `datasync` is true, then only the user data should be
    /// flushed, not the metadata. when `path` is None, it means the path may be deleted.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn fsync(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        datasync: bool,
    ) -> FuseResult<()> {
        self.fs.flush_write_buffer().await?;

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidFileHandle(fh).into());
            };
            p
        };

        self.fs.cache_invalidate(&file_path);
        Ok(())
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
    ) -> FuseResult<ReplyCopyFileRange> {
        let file_in_path = if let Some(p) = from_path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh_in).await else {
                return Err(FuseError::InvalidFileHandle(fh_in).into());
            };
            p
        };

        let file_out_path = if let Some(p) = to_path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh_out).await else {
                return Err(FuseError::InvalidFileHandle(fh_out).into());
            };
            p
        };

        let data = self
            .fs
            .read_file(
                &file_in_path,
                offset_in as usize,
                length as usize,
                // &self.auth_token,
            )
            .await?;

        let fs_flags = fs_model::Flags::try_from(flags)?;

        let write_data = self
            .fs
            .write_file(
                &file_out_path,
                &fs_flags,
                offset_out as usize,
                data.as_slice(),
                // &self.auth_token,
            )
            .await?;

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
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn fallocate(
        &self,
        req: Request,
        path: Option<&OsStr>,
        fh: u64,
        offset: u64,
        length: u64,
        mode: u32,
    ) -> FuseResult<()> {
        // TODO:
        // Err(FuseError::NotImplemented.into())

        let file_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            let Ok(Some(p)) = self.fs.get_path_from_fh(fh).await else {
                return Err(FuseError::InvalidFileHandle(fh).into());
            };
            p
        };

        let fs_type = fs_model::FileType::try_from(mode)?;

        self.fs
            .create_file(
                &file_path,
                &fs_type,
                offset as usize,
                &Vec::with_capacity(length as usize),
                // &self.auth_token,
            )
            .await?;

        Ok(())
    }

    /// create a directory.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn mkdir(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> FuseResult<ReplyEntry> {
        let parent_path = Path::new(parent);
        let complete_path = parent_path.join(name);

        let attr = self.fs.mkdir(&complete_path).await?;

        Ok(ReplyEntry {
            ttl: self.fs.get_ttl(),
            attr: attr.into(),
        })
    }

    /// remove a directory.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn rmdir(&self, req: Request, parent: &OsStr, name: &OsStr) -> FuseResult<()> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())
        let path = Path::new(parent).join(name);

        self.fs.remove(&path).await?;
        Ok(())
    }

    /// open a directory. Filesystem may store an arbitrary file handle (pointer, index, etc) in
    /// `fh`, and use this in other all other directory stream operations
    /// ([`readdir`][PathFilesystem::readdir], [`releasedir`][PathFilesystem::releasedir],
    /// [`fsyncdir`][PathFilesystem::fsyncdir]). Filesystem may also implement stateless directory
    /// I/O and not store anything in `fh`.  A file system need not implement this method if it
    /// sets [`MountOptions::no_open_dir_support`][crate::MountOptions::no_open_dir_support] and if
    /// the kernel supports `FUSE_NO_OPENDIR_SUPPORT`.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn opendir(&self, req: Request, path: &OsStr, flags: u32) -> FuseResult<ReplyOpen> {
        let path = Path::new(path);
        let fs_flags = fs_model::Flags::try_from(flags)?;

        /* not needed for directories
        if fs_flags.create || fs_flags.excl || fs_flags.noctt {
            return Err(FuseError::InvalidInput("Invalid open flags".to_string()).into());
        }
        */

        let fh = self.fs.open(path, &fs_flags).await?;

        Ok(ReplyOpen { fh, flags })
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
    ) -> FuseResult<ReplyDirectory<Self::DirEntryStream<'a>>> {
        let path = Path::new(path);
        let items = self.fs.readdir(path).await?;

        let entries: Vec<FuseResult<DirectoryEntry>> = items
            .into_iter()
            .skip(offset as usize)
            .enumerate()
            .map(|(idx, item)| {
                let kind = match item.item_type {
                    ItemType::File => FileType::RegularFile,
                    ItemType::SymLink => FileType::Symlink,
                    ItemType::Directory => FileType::Directory,
                };
                Ok(DirectoryEntry {
                    offset: (offset + idx as i64 + 1),
                    name: item.name.into(),
                    kind,
                })
            })
            .collect();

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
    ) -> FuseResult<ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        // TODO:
        let path = Path::new(parent);

        let items = self.fs.readdir(&path).await?;

        let entries: Vec<FuseResult<DirectoryEntryPlus>> = items
            .into_iter()
            .skip(offset as usize)
            .enumerate()
            .map(|(idx, item)| {
                let (kind, attr) = match item.item_type {
                    ItemType::File => (FileType::RegularFile, item.attributes),
                    ItemType::SymLink => (FileType::Symlink, item.attributes),
                    ItemType::Directory => (FileType::Directory, item.attributes),
                };
                Ok(DirectoryEntryPlus {
                    kind,
                    name: item.name.into(),
                    offset: (offset + idx as u64 + 1) as i64,
                    attr: attr.into(),
                    entry_ttl: self.fs.get_ttl(),
                    attr_ttl: self.fs.get_ttl(),
                })
            })
            .collect();

        let stream = stream::iter(entries);
        Ok(ReplyDirectoryPlus { entries: stream })
    }

    /// release an open directory. For every [`opendir`][PathFilesystem::opendir] call there will
    /// be exactly one `releasedir` call. `fh` will contain the value set by the
    /// [`opendir`][PathFilesystem::opendir] method, or will be undefined if the
    /// [`opendir`][PathFilesystem::opendir] method didn\'t set any value.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn releasedir(&self, req: Request, path: &OsStr, fh: u64, flags: u32) -> FuseResult<()> {
        let file_path = PathBuf::from(path);

        let fs_flags = fs_model::Flags::try_from(flags)?;

        self.fs.release(&file_path, &fs_flags, fh).await?;

        Ok(())
    }

    /// synchronize directory contents. If the `datasync` is true, then only the directory contents
    /// should be flushed, not the metadata. `fh` will contain the value set by the
    /// [`opendir`][PathFilesystem::opendir] method, or will be undefined if the
    /// [`opendir`][PathFilesystem::opendir] method didn\'t set any value.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn fsyncdir(
        &self,
        req: Request,
        path: &OsStr,
        fh: u64,
        datasync: bool,
    ) -> FuseResult<()> {
        let path = Path::new(path);
        self.fs.cache_invalidate(path);
        Ok(())
    }

    /// rename a file or directory.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn rename(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
    ) -> FuseResult<()> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        // Err(libc::ENOSYS.into())
        let old_path = Path::new(origin_parent).join(origin_name);
        let new_path = Path::new(parent).join(name);

        self.fs.rename(&old_path, &new_path).await?;
        Ok(())
    }

    /// rename a file or directory with flags.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn rename2(
        &self,
        req: Request,
        origin_parent: &OsStr,
        origin_name: &OsStr,
        parent: &OsStr,
        name: &OsStr,
        flags: u32,
    ) -> FuseResult<()> {
        // TODO:
        // tracing::warn!("[Not Implemented]");
        //Err(libc::ENOSYS.into());
        let old_path = Path::new(origin_parent).join(origin_name);
        let new_path = Path::new(parent).join(name);

        self.fs.rename(&old_path, &new_path).await?;
        Ok(())
    }

    /// create a hard link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn link(
        &self,
        req: Request,
        path: &OsStr,
        new_parent: &OsStr,
        new_name: &OsStr,
    ) -> FuseResult<ReplyEntry> {
        // TODO:
        Err(FuseError::Unsupported("link".to_string()).into())
    }

    /// remove a file.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn unlink(&self, req: Request, parent: &OsStr, name: &OsStr) -> FuseResult<()> {
        let path = Path::new(parent).join(name);

        self.fs.remove(&path).await?;
        Ok(())
    }

    /// read symbolic link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn readlink(&self, req: Request, path: &OsStr) -> FuseResult<ReplyData> {
        let path = Path::new(path);
        let target = self.fs.read_symlink(path).await?;

        Ok(ReplyData {
            data: target.into_bytes().into(),
        })
    }

    /// create a symbolic link.
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn symlink(
        &self,
        req: Request,
        parent: &OsStr,
        name: &OsStr,
        link_path: &OsStr,
    ) -> FuseResult<ReplyEntry> {
        let path = Path::new(parent).join(name);
        let Some(target) = link_path.to_str() else {
            return Err(FuseError::InvalidInput("Invalid symlink target".to_string()).into());
        };
        let file_attr = self.fs.create_symlink(&path, target.as_ref()).await?;

        Ok(ReplyEntry {
            ttl: self.fs.get_ttl(),
            attr: file_attr.into(),
        })
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
    ) -> FuseResult<ReplyLock> {
        // TODO:
        Err(FuseError::NotImplemented.into())
    }

    /// acquire, modify or release a POSIX file lock."]
    ///
    /// # Notes:
    ///
    /// this is supported on enable **`file-lock`** feature.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), err(level = Level::ERROR))]
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
    ) -> FuseResult<()> {
        // TODO:
        Err(FuseError::NotImplemented.into())
    }

    /// handle interrupt. When a operation is interrupted, an interrupt request will send to fuse
    /// server with the unique id of the operation.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn interrupt(&self, req: Request, unique: u64) -> FuseResult<()> {
        // TODO:
        Err(FuseError::Unsupported("interrupt".to_string()).into())
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
    ) -> FuseResult<ReplyPoll> {
        // TODO:
        Err(FuseError::Unsupported("poll".to_string()).into())
    }

    /// receive notify reply from kernel.
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn notify_reply(
        &self,
        req: Request,
        path: &OsStr,
        offset: u64,
        data: Bytes,
    ) -> FuseResult<()> {
        // TODO:
        Err(FuseError::Unsupported("notify_reply".to_string()).into())
    }

    /// forget more than one path. This is a batch version [`forget`][PathFilesystem::forget]
    #[instrument(skip(self))]
    async fn batch_forget(&self, req: Request, paths: &[&OsStr]) -> () {
        for path in paths {
            let path = Path::new(path);
            self.fs.cache_invalidate(path);
        }
    }
}

impl From<fs_model::Attributes> for FileAttr {
    fn from(value: fs_model::Attributes) -> Self {
        Self {
            size: value.size,
            blocks: value.blocks,
            atime: value.atime.into(),
            mtime: value.mtime.into(),
            ctime: value.ctime.into(),
            kind: value.kind.into(),
            perm: u16::from(value.kind) | u16::from(value.perm),
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

impl From<SetAttr> for fs_model::SetAttr {
    fn from(value: SetAttr) -> Self {
        Self {
            uid: value.uid,
            gid: value.gid,
            size: value.size,
            lock_owner: value.lock_owner,

            // Conversione del mode (permessi)
            // Nota: Assumiamo che il mode di fuser sia un u32 che rappresenta i permessi POSIX
            mode: value.mode,

            // Conversione di SystemTime in Timestamp
            atime: value.atime.map(fs_model::attributes::Timestamp::from),
            mtime: value.mtime.map(fs_model::attributes::Timestamp::from),
            ctime: value.ctime.map(fs_model::attributes::Timestamp::from),
        }
    }
}

impl From<fuse3::Timestamp> for fs_model::Timestamp {
    fn from(t: fuse3::Timestamp) -> Self {
        fs_model::Timestamp {
            sec: t.sec,
            nsec: t.nsec,
        }
    }
}

impl From<FuseError> for Errno {
    fn from(value: FuseError) -> Self {
        tracing::error!("{}", value);
        match value {
            // --- Authentication ---
            FuseError::Unauthorized(_) => libc::EACCES.into(),
            // --- File and Path ---
            FuseError::NotFound(_) => libc::ENOENT.into(),
            FuseError::AlreadyExists(_) => libc::EEXIST.into(),
            FuseError::NotADirectory(_) => libc::ENOTDIR.into(),
            FuseError::IsADirectory(_) => libc::EISDIR.into(),
            // --- Permission and Security ---
            FuseError::PermissionDenied(_) => libc::EACCES.into(),
            FuseError::OperationNotPermitted(_) => libc::EPERM.into(),
            // --- Space and Resources ---
            FuseError::StorageFull(_) => libc::ENOSPC.into(),
            FuseError::OutOfMemory(_) => libc::ENOMEM.into(),
            // --- Arguments and State ---
            FuseError::InvalidInput(_) => libc::EINVAL.into(),
            FuseError::FileTooLarge(_) => libc::EOVERFLOW.into(),
            // --- Unsupported Operations ---
            FuseError::Unsupported(_) => libc::EOPNOTSUPP.into(),
            FuseError::CrossDeviceLink(_) => libc::EXDEV.into(),
            // --- I/O and Consistency ---
            FuseError::IoError(_) => libc::EIO.into(),
            FuseError::TextFileBusy(_) => libc::ETXTBSY.into(),
            // --- Lock and Concurrency ---
            FuseError::ResourceBusy(_) => libc::EBUSY.into(),
            FuseError::TryAgain(_) => libc::EAGAIN.into(),
            // --- Other ---
            FuseError::InternalError(_) => libc::EIO.into(),
            FuseError::NotImplemented => libc::ENOSYS.into(),
            FuseError::InvalidFileHandle(_) => libc::EBADF.into(),
        }
    }
}

impl From<FsModelError> for Errno {
    fn from(value: FsModelError) -> Self {
        tracing::error!("{}", value);
        match value {
            FsModelError::NotFound(_) => libc::ENOENT.into(),
            FsModelError::PermissionDenied(_) => libc::EACCES.into(),
            FsModelError::InvalidInput(_) => libc::EINVAL.into(),
            FsModelError::ConversionFailed(_) => libc::EINVAL.into(),
            FsModelError::FileHandlerError => libc::EIO.into(),
            FsModelError::WriterError => libc::EIO.into(),
            FsModelError::NoData(_) => libc::ENODATA.into(),
            FsModelError::ServerError(net_err) => Errno::from(net_err),
            FsModelError::Other(_) => libc::EIO.into(),
        }
    }
}

impl From<NetworkError> for Errno {
    fn from(value: NetworkError) -> Self {
        match value {
            NetworkError::ConnectionFailed(_) => libc::ECONNREFUSED.into(),
            NetworkError::InvalidInput(_) => libc::EINVAL.into(),
            NetworkError::Request(_) => libc::EIO.into(),
            NetworkError::ServerError(api_err) => Errno::from(api_err),
            NetworkError::UnexpectedResponse(_) => libc::EIO.into(),
            NetworkError::Other(_) => libc::EIO.into(),
        }
    }
}
