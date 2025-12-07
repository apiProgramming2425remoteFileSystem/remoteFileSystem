use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::FsModelError;

/// File attributes
#[derive(Debug, Copy, Clone, Deserialize /*, Ord, PartialOrd, Eq, PartialEq, Hash */)]
pub struct FileAttr {
    /// Size in bytes
    pub size: u64,
    /// Size in blocks
    pub blocks: u64,
    /// Time of last access
    pub atime: Timestamp,
    /// Time of last modification
    pub mtime: Timestamp,
    /// Time of last change
    pub ctime: Timestamp,
    /// Time of creation (macOS only)
    pub crtime: Timestamp,
    /// Kind of file (directory, file, pipe, etc)
    pub kind: FileType,
    /// Permissions
    pub perm: Permission,
    /// Number of hard links
    pub nlink: u32,
    /// User id
    pub uid: u32,
    /// Group id
    pub gid: u32,
    /// Rdev
    pub rdev: u32,
    /// block size
    pub blksize: u32,
    /// #[cfg(target_os = "macos")]
    pub flags: u32,
}

/// File types
#[derive(Debug, Copy, Clone, Deserialize, Eq, PartialEq /*, Ord, PartialOrd, Hash */)]
pub enum FileType {
    /// Named pipe [`libc::S_IFIFO`]
    NamedPipe,
    /// Character device [`libc::S_IFCHR`]
    CharDevice,
    /// Block device [`libc::S_IFBLK`]
    BlockDevice,
    /// Directory [`libc::S_IFDIR`]
    Directory,
    /// Regular file [`libc::S_IFREG`]
    RegularFile,
    /// Symbolic link [`libc::S_IFLNK`]
    Symlink,
    /// Unix domain socket [`libc::S_IFSOCK`]
    Socket,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Permission type
pub struct PermissionType {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execution permission
    pub execute: bool,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Permission
pub struct Permission {
    /// Permissions for the file owner.
    pub user: PermissionType,
    /// Permissions for the group owner.
    pub group: PermissionType,
    /// Permissions for all others.
    pub other: PermissionType,
}

#[derive(Debug)]
/// Flags
pub struct Flags {
    /// Read-only access. [`libc::O_RDONLY`]
    pub readonly: bool,
    /// Write-only access. [`libc::O_WRONLY`]
    pub writeonly: bool,
    /// Read and Write access. [`libc::O_RDWR`]
    pub readwrite: bool,

    /// Create if it does not exist. [`libc::O_CREAT`]
    pub create: bool,
    /// Fail if exists. [`libc::O_EXCL`]
    pub excl: bool,
    /// Truncate to zero length if it exists [`libc::O_TRUNC`].
    pub trunc: bool,
    /// Append mode. [`libc::O_APPEND`]
    pub append: bool,

    /// Non-blocking mode. [`libc::O_NONBLOCK`] | [`libc::O_NDELAY`]
    pub nonblock: bool,
    /// Do not assign controlling terminal. [`libc::O_NOCTTY`]
    pub noctt: bool,
    /// Synchronized I/O. [`libc::O_SYNC`] | [`libc::O_FSYNC`] | [`libc::O_RSYNC`]
    pub sync: bool,
    /// Data synchronized writes only. [`libc::O_DSYNC`]
    pub dsync: bool,

    /// Fail if not a directory. [`libc::O_DIRECTORY`]
    pub directory: bool,
    /// Do not follow symbolic links. [`libc::O_NOFOLLOW`]
    pub nofollow: bool,
    /// Set close-on-exec flag for the file descriptor. [`libc::O_CLOEXEC`]
    pub cloexec: bool,
    /// Create unnamed temporary file. [`libc::O_TMPFILE`]
    pub tmpfile: bool,

    /// Enable signal-driven I/O. [`libc::O_ASYNC`]
    pub async_io: bool,
    /// Minimize cache effects of I/O. [`libc::O_DIRECT`]
    pub direct: bool,
    /// Do not update access timestamp on reads. [`libc::O_NOATIME`]
    pub noatime: bool,
    /// Obtain a file descriptor without opening file. [`libc::O_PATH`]
    pub path: bool,
}

/// A file's timestamp, according to FUSE.
#[derive(Debug, Clone, Serialize, Deserialize, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Timestamp {
    pub sec: i64,
    pub nsec: u32,
}

impl Timestamp {
    /// Create a new timestamp from its component parts.
    ///
    /// `nsec` should be less than 1_000_000_000.
    pub fn new(sec: i64, nsec: u32) -> Self {
        Timestamp { sec, nsec }
    }
}

impl From<SystemTime> for Timestamp {
    fn from(t: SystemTime) -> Self {
        let d = t
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        Timestamp {
            sec: d.as_secs().try_into().unwrap_or(i64::MAX),
            nsec: d.subsec_nanos(),
        }
    }
}

impl From<Timestamp> for SystemTime {
    fn from(t: Timestamp) -> Self {
        let duration = Duration::new(t.sec as u64, t.nsec);
        UNIX_EPOCH.checked_add(duration).unwrap()
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize)]
pub struct SetAttr {
    /// set file or directory mode.
    pub mode: Option<Permission>,
    /// set file or directory uid.
    pub uid: Option<u32>,
    /// set file or directory gid.
    pub gid: Option<u32>,
    /// set file or directory size.
    pub size: Option<u64>,
    /// the lock_owner argument.
    pub lock_owner: Option<u64>,
    /// set file or directory atime.
    pub atime: Option<Timestamp>,
    /// set file or directory mtime.
    pub mtime: Option<Timestamp>,
    /// set file or directory ctime.
    pub ctime: Option<Timestamp>,
    #[cfg(target_os = "macos")]
    pub crtime: Option<Timestamp>,
    #[cfg(target_os = "macos")]
    pub chgtime: Option<Timestamp>,
    #[cfg(target_os = "macos")]
    pub bkuptime: Option<Timestamp>,
    #[cfg(target_os = "macos")]
    pub flags: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Stats {
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub bsize: u32,
    pub namelen: u32,
    pub frsize: u32,
}

trait Conversion<T>: Sized {
    type Error;

    fn from_target(&self) -> T;
    fn try_to_target(value: T) -> Result<Self, Self::Error>;
}

macro_rules! impl_conversion {
    ($source:ty, $target:ty, $cv:ty) => {
        impl From<$source> for $target
        where
            $source: Conversion<$cv>,
        {
            fn from(value: $source) -> Self {
                value.from_target() as $target
            }
        }

        impl TryFrom<$target> for $source
        where
            $source: Conversion<$cv>,
        {
            type Error = FsModelError;

            fn try_from(value: $target) -> Result<Self, Self::Error> {
                Self::try_to_target(value as $cv)
            }
        }
    };
}

impl_conversion!(FileType, u32, u32);
impl_conversion!(FileType, i32, u32);
impl_conversion!(FileType, u16, u32);

impl_conversion!(PermissionType, i32, i32);
impl_conversion!(PermissionType, u8, i32);

impl_conversion!(Permission, u16, i32);
impl_conversion!(Permission, u32, i32);

impl_conversion!(Flags, u32, i32);
impl_conversion!(Flags, u64, i32);

#[cfg(unix)]
mod platform {
    use super::*;
    use libc;

    impl Conversion<u32> for FileType {
        type Error = FsModelError;

        fn from_target(&self) -> u32 {
            match self {
                FileType::NamedPipe => libc::S_IFIFO,
                FileType::CharDevice => libc::S_IFCHR,
                FileType::BlockDevice => libc::S_IFBLK,
                FileType::Directory => libc::S_IFDIR,
                FileType::RegularFile => libc::S_IFREG,
                FileType::Symlink => libc::S_IFLNK,
                FileType::Socket => libc::S_IFSOCK,
            }
        }

        fn try_to_target(value: u32) -> Result<Self, Self::Error> {
            match value & libc::S_IFMT {
                libc::S_IFIFO => Ok(FileType::NamedPipe),
                libc::S_IFCHR => Ok(FileType::CharDevice),
                libc::S_IFBLK => Ok(FileType::BlockDevice),
                libc::S_IFDIR => Ok(FileType::Directory),
                libc::S_IFREG => Ok(FileType::RegularFile),
                libc::S_IFLNK => Ok(FileType::Symlink),
                libc::S_IFSOCK => Ok(FileType::Socket),
                _ => Err(FsModelError::ConversionFailed),
            }
        }
    }

    impl Conversion<i32> for PermissionType {
        type Error = FsModelError;

        fn from_target(&self) -> i32 {
            (if self.read { libc::R_OK } else { 0 })
                | (if self.write { libc::W_OK } else { 0 })
                | (if self.execute { libc::X_OK } else { 0 })
        }

        fn try_to_target(value: i32) -> Result<Self, Self::Error> {
            Ok(Self {
                read: value & libc::R_OK != 0,
                write: value & libc::W_OK != 0,
                execute: value & libc::X_OK != 0,
            })
        }
    }

    impl Conversion<i32> for Permission {
        type Error = FsModelError;

        fn from_target(&self) -> i32 {
            i32::from(self.user) << 6 | i32::from(self.group) << 3 | i32::from(self.other)
        }

        fn try_to_target(value: i32) -> Result<Self, Self::Error> {
            Ok(Self {
                user: (value >> 6).try_into()?,
                group: (value >> 3).try_into()?,
                other: (value).try_into()?,
            })
        }
    }

    impl Conversion<i32> for Flags {
        type Error = FsModelError;

        fn from_target(&self) -> i32 {
            (if self.readonly { libc::O_RDONLY } else { 0 })
                | (if self.writeonly { libc::O_WRONLY } else { 0 })
                | (if self.readwrite { libc::O_RDWR } else { 0 })
                | (if self.create { libc::O_CREAT } else { 0 })
                | (if self.excl { libc::O_EXCL } else { 0 })
                | (if self.trunc { libc::O_TRUNC } else { 0 })
                | (if self.append { libc::O_APPEND } else { 0 })
                | (if self.nonblock { libc::O_NONBLOCK } else { 0 })
                | (if self.noctt { libc::O_NOCTTY } else { 0 })
                | (if self.sync { libc::O_SYNC } else { 0 })
                | (if self.dsync { libc::O_DSYNC } else { 0 })
                | (if self.directory { libc::O_DIRECTORY } else { 0 })
                | (if self.nofollow { libc::O_NOFOLLOW } else { 0 })
                | (if self.cloexec { libc::O_CLOEXEC } else { 0 })
                | (if self.tmpfile { libc::O_TMPFILE } else { 0 })
                | (if self.async_io { libc::O_ASYNC } else { 0 })
                | (if self.direct { libc::O_DIRECT } else { 0 })
                | (if self.noatime { libc::O_NOATIME } else { 0 })
                | (if self.path { libc::O_PATH } else { 0 })
        }

        fn try_to_target(value: i32) -> Result<Self, Self::Error> {
            tracing::debug!(
                "flags contains FUSE_WRITE_CACHE: {}",
                (value as u32) & fuse3::raw::flags::FUSE_WRITE_CACHE != 0
            );
            Ok(Self {
                readonly: value & libc::O_RDONLY != 0,
                writeonly: value & libc::O_WRONLY != 0,
                readwrite: value & libc::O_RDWR != 0,
                create: value & libc::O_CREAT != 0,
                excl: value & libc::O_EXCL != 0,
                trunc: value & libc::O_TRUNC != 0,
                append: value & libc::O_APPEND != 0,
                nonblock: value & libc::O_NONBLOCK != 0,
                noctt: value & libc::O_NOCTTY != 0,
                sync: value & libc::O_SYNC != 0,
                dsync: value & libc::O_DSYNC != 0,
                directory: value & libc::O_DIRECTORY != 0,
                nofollow: value & libc::O_NOFOLLOW != 0,
                cloexec: value & libc::O_CLOEXEC != 0,
                tmpfile: value & libc::O_TMPFILE != 0,
                async_io: value & libc::O_ASYNC != 0,
                direct: value & libc::O_DIRECT != 0,
                noatime: value & libc::O_NOATIME != 0,
                path: value & libc::O_PATH != 0,
            })
        }
    }
}
