use fuser::{Filesystem, Request, ReplyEntry, ReplyAttr, ReplyDirectory, FileAttr, FileType};
use libc::ENOENT;
use std::time::{Duration, SystemTime};
use std::ffi::OsStr;
use super::network::models::ItemType;
use super::network::client::list_path;

const TTL: Duration = Duration::from_secs(1); // cache timeout breve

fn default_attr(ino: u64, kind: FileType) -> FileAttr {
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        kind,
        perm: 0o755,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}


pub struct SimpleFS;

impl Filesystem for SimpleFS {
    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        // mock: inode 1 è la root directory, tutto il resto file
        if ino == 1 {
            reply.attr(&TTL, &default_attr(ino, FileType::Directory));
        } else {
            reply.attr(&TTL, &default_attr(ino, FileType::RegularFile));
        }
    }

    fn lookup(&mut self, _req: &Request<'_>, _parent: u64, _name: &std::ffi::OsStr, reply: ReplyEntry) {
        // mock: qualunque file trovato diventa un "file fittizio"
        let attr = default_attr(2, FileType::RegularFile);
        reply.entry(&TTL, &attr, 0);
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        println!("I'm here");
        let result = match list_path("/") {
            Some(items) => items,
            None => return reply.error(ENOENT)
        };
        println!("result was ok");
        let mut offset_idx = 1;
        if offset <= offset_idx {
            reply.add(1, offset_idx, FileType::Directory, ".");
        }
        offset_idx += 1;
        if offset <= offset_idx {
            reply.add(1, offset_idx, FileType::Directory, "..");
        }
        offset_idx += 1;
        println!(". and .. done");

        for (i, item) in result.iter().enumerate().skip(offset as usize){
            println!("item number {}", i);
            let kind = match item.item_type {
                ItemType::Directory => FileType::Directory,
                ItemType::File => FileType::RegularFile
            };
            let child_ino = 2 + i as u64;
            reply.add(child_ino, offset_idx+i as i64, kind, item.name.as_str());
        }
        reply.ok();
    }

    fn destroy(&mut self) {
        println!("Filesystem unmounted");
    }
}