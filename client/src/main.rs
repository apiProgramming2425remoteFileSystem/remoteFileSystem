use fuser::{Filesystem, Request, ReplyEntry, ReplyAttr, ReplyDirectory, FileAttr, FileType, MountOption};
use libc::ENOENT;
use std::time::{Duration, SystemTime};
use std::ffi::OsStr;

const TTL: Duration = Duration::from_secs(1);

const ROOT_DIR_ATTR: FileAttr = FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: SystemTime::UNIX_EPOCH,
    mtime: SystemTime::UNIX_EPOCH,
    ctime: SystemTime::UNIX_EPOCH,
    crtime: SystemTime::UNIX_EPOCH,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 1000,
    rdev: 0,
    blksize: 512,
    flags: 0,
};

const HELLO_FILE_ATTR: FileAttr = FileAttr {
    ino: 2,
    size: 12, // "Hello World\n"
    blocks: 1,
    atime: SystemTime::UNIX_EPOCH,
    mtime: SystemTime::UNIX_EPOCH,
    ctime: SystemTime::UNIX_EPOCH,
    crtime: SystemTime::UNIX_EPOCH,
    kind: FileType::RegularFile,
    perm: 0o644,
    nlink: 1,
    uid: 1000,
    gid: 1000,
    rdev: 0,
    blksize: 512,
    flags: 0,
};

struct SimpleFS;

impl Filesystem for SimpleFS {
    fn lookup(&mut self, _req: &Request, _parent: u64, name: &OsStr, reply: ReplyEntry) {
        if name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &HELLO_FILE_ATTR, 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        match ino {
            1 => reply.attr(&TTL, &ROOT_DIR_ATTR),
            2 => reply.attr(&TTL, &HELLO_FILE_ATTR),
            _ => reply.error(ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino != 1 {
            reply.error(ENOENT);
            return;
        }

        if offset == 0 {
            reply.add(1, 1, FileType::Directory, ".");
            reply.add(1, 2, FileType::Directory, "..");
            reply.add(2, 3, FileType::RegularFile, "hello.txt");
        }

        reply.ok();
    }

    fn destroy(&mut self) {
        println!("Filesystem unmounted");
    }
}

fn main() {
    let mountpoint = std::env::args().nth(1).expect("Usage: fuse-test <MOUNTPOINT>");
    fuser::mount2(SimpleFS, mountpoint, &[MountOption::RO]).unwrap();
}
