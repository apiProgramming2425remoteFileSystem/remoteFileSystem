use std::time::SystemTime;

use libc;

/// File attributes
#[derive(Debug, Copy, Clone /*, Ord, PartialOrd, Eq, PartialEq, Hash */)]
pub struct FileAttr {
    /// Size in bytes
    pub size: u64,
    /// Size in blocks
    pub blocks: u64,
    /// Time of last access
    pub atime: SystemTime,
    /// Time of last modification
    pub mtime: SystemTime,
    /// Time of last change
    pub ctime: SystemTime,
    #[cfg(target_os = "macos")]
    /// Time of creation (macOS only)
    pub crtime: SystemTime,
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
    #[cfg(target_os = "macos")]
    /// Flags (macOS only, see chflags(2))
    pub flags: u32,
}

/// File types
#[derive(Debug, Copy, Clone /*, Ord, PartialOrd, Eq, PartialEq, Hash */)]
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

impl FileType {
    fn from_file_type(&self) -> u32 {
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
    fn try_to_file_type(value: u32) -> Result<Self, ()> {
        match value & libc::S_IFMT {
            libc::S_IFIFO => Ok(FileType::NamedPipe),
            libc::S_IFCHR => Ok(FileType::CharDevice),
            libc::S_IFBLK => Ok(FileType::BlockDevice),
            libc::S_IFDIR => Ok(FileType::Directory),
            libc::S_IFREG => Ok(FileType::RegularFile),
            libc::S_IFLNK => Ok(FileType::Symlink),
            libc::S_IFSOCK => Ok(FileType::Socket),
            _ => Err(()),
        }
    }
}

impl From<FileType> for u32 {
    fn from(value: FileType) -> Self {
        value.from_file_type()
    }
}
impl From<FileType> for i32 {
    fn from(value: FileType) -> Self {
        value.from_file_type() as i32
    }
}
impl TryFrom<u32> for FileType {
    type Error = (); // TODO: Define an Error Type 

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_to_file_type(value)
    }
}
impl TryFrom<i32> for FileType {
    type Error = (); // TODO: Define an Error Type 

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::try_to_file_type(value as u32)
    }
}

#[derive(Debug, Copy, Clone)]
/// permission type
pub struct PermissionType {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execution permission
    pub execute: bool,
}

impl PermissionType {
    fn from_permission(&self) -> i32 {
        (if self.read { libc::R_OK } else { 0 })
            | (if self.write { libc::W_OK } else { 0 })
            | (if self.execute { libc::X_OK } else { 0 })
    }
    fn to_permission(value: i32) -> Self {
        Self {
            read: value & libc::R_OK != 0,
            write: value & libc::W_OK != 0,
            execute: value & libc::X_OK != 0,
        }
    }
}

impl From<PermissionType> for i32 {
    fn from(value: PermissionType) -> Self {
        value.from_permission()
    }
}
impl From<i32> for PermissionType {
    fn from(value: i32) -> Self {
        Self::to_permission(value)
    }
}

#[derive(Debug, Copy, Clone)]
/// Permission
pub struct Permission {
    /// The kind of file
    pub file_type: FileType,
    /// Permissions for the file owner.
    pub user: PermissionType,
    /// Permissions for the group owner.
    pub group: PermissionType,
    /// Permissions for all others.
    pub other: PermissionType,
}

impl Permission {
    fn from_permission(&self) -> i32 {
        i32::from(self.file_type)
            | i32::from(self.user) << 6
            | i32::from(self.group) << 3
            | i32::from(self.other)
    }
    // TODO: replace Err with the same type as the TryFrom
    fn try_to_permission(value: i32) -> Result<Self, ()> {
        Ok(Self {
            file_type: FileType::try_from(value)?,
            user: PermissionType::from(value >> 6),
            group: PermissionType::from(value >> 3),
            other: PermissionType::from(value),
        })
    }
}

impl From<Permission> for u16 {
    fn from(value: Permission) -> Self {
        return value.from_permission() as u16;
    }
}
impl TryFrom<u16> for Permission {
    type Error = (); // TODO: Define an Error Type

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_to_permission(value as i32)
    }
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

impl Flags {
    fn from_flags(&self) -> i32 {
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

    fn to_flags(value: i32) -> Self {
        Self {
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
        }
    }
}

impl From<Flags> for u32 {
    fn from(value: Flags) -> Self {
        return value.from_flags() as u32;
    }
}
impl From<u32> for Flags {
    fn from(value: u32) -> Self {
        Self::to_flags(value as i32)
    }
}
impl From<u64> for Flags {
    fn from(value: u64) -> Self {
        Self::to_flags(value as i32)
    }
}
