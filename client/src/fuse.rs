use std::ffi::{OsStr};
use std::num::NonZeroU32;
use fuse3::path::prelude::*;
use fuse3::{Errno, Result};
use std::time::SystemTime;
use std::time::Duration;
use futures_util::stream;
use libc;

use crate::config::Config;
use crate::network::client::RemoteClient;
use crate::network;
use crate::network::models::{ItemType, SerializableFSItem};

const TTL: Duration = Duration::from_secs(1);
const SEPARATOR: char = '/';


#[derive(Debug)]
pub struct Fs{
    config: Config,
    remoteClient: RemoteClient,
}

impl Fs{
    pub fn new(config: Config) -> Self{
        let base_url = config.server_url.clone() + network::APP_V1_BASE_URL;
        let remoteClient = RemoteClient::new(base_url);
        Fs {config, remoteClient}
    }

    async fn fetch_list_path(&self, path: &OsStr) -> anyhow::Result<Vec<SerializableFSItem>> {
        self.remoteClient.list_path(path).await
    }

    fn mock_file_attr(&self) -> FileAttr {
        FileAttr {
            size: 0,
            blocks: 0,
            blksize: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: 1,
            gid: 1,
            rdev: 0,
        }
    }
}

impl PathFilesystem for Fs {
    type DirEntryStream<'a> =
        futures_util::stream::Iter<std::vec::IntoIter<Result<DirectoryEntry>>>
        where Self: 'a;
    type DirEntryPlusStream<'a> =
        futures_util::stream::Iter<std::vec::IntoIter<Result<DirectoryEntryPlus>>>
        where Self: 'a;

    async fn init(&self, _req: Request) -> Result<ReplyInit> {
        Ok(ReplyInit {
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
            max_write: NonZeroU32::new(64 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: Request) {}

    async fn readdir<'a>(
        &'a self,
        _req: Request,
        path: &'a OsStr,
        _fh: u64,
        offset: i64,
    ) -> Result<ReplyDirectory<Self::DirEntryStream<'a>>> {
        println!("readdir");

        let items = match self.fetch_list_path(path).await {
            Ok(vec_items) => vec_items,
            Err(err) => {
                tracing::error!("fetch_list_path failed: {err}");
                return Err(Errno::from(libc::EIO)); //  generic I/O error
            }
        };

        let entries: Vec<Result<DirectoryEntry>> = items.into_iter()
            .skip(offset as usize)
            .enumerate()
            .map(|(idx, item)| {
                let kind = match item.item_type {
                    ItemType::File => FileType::RegularFile,
                    ItemType::Directory => FileType::Directory,
                };
                Ok(DirectoryEntry {
                    offset: (offset + idx as i64),
                    name: item.name.into(),
                    kind,
                })
            })
            .collect();

        let stream = stream::iter(entries);
        Ok(ReplyDirectory { entries: stream })
    }

    async fn readdirplus<'a>(
        &'a self,
        _req: Request,
        path: &'a OsStr,
        _fh: u64,
        offset: u64,
        _lock_owner: u64,
    ) -> Result<ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        println!("readdirplus");

        let items = match self.fetch_list_path(path).await {
            Ok(vec_items) => vec_items,
            Err(err) => {
                tracing::error!("fetch_list_path failed: {err}");
                return Err(Errno::from(libc::EIO));
            }
        };

        let entries: Vec<Result<DirectoryEntryPlus>> = items.into_iter()
            .skip(offset as usize)
            .enumerate()
            .map(|(idx, item)| {
                let kind = match item.item_type {
                    ItemType::File => FileType::RegularFile,
                    ItemType::Directory => FileType::Directory,
                };
                Ok(DirectoryEntryPlus {
                    kind,
                    name: item.name.into(),
                    offset: (offset + idx as u64) as i64,
                    attr: self.mock_file_attr(),
                    entry_ttl: TTL,
                    attr_ttl: TTL,
                })
            })
            .collect();

        let stream = stream::iter(entries);
        Ok(ReplyDirectoryPlus { entries: stream })
    }

    async fn opendir(
        &self,
        _req: Request,
        _path: &OsStr,
        _flags: u32,
        ) -> Result<ReplyOpen> {
        println!("opendir");
        Ok(ReplyOpen {
            fh: 1,  
            flags: 0,
        })
    }

    async fn releasedir(
        &self,
        _req: Request,
        _path: &OsStr,
        _fh: u64,
        _flags: u32,
    ) -> Result<()> {
        println!("releasedir");
        Ok(())
    }


    /*
    
    è solo un mock per far funzionare le altre cose...
    restituiamo un valore di default a caso, i dati veri saranno da collegare poi
    
     */
    
    async fn getattr(
        &self,
        _req: Request,
        _path: Option<&OsStr>,
        _fh: Option<u64>,
        _flags: u32,
    ) -> Result<ReplyAttr> {
        println!("gettattr");
        let attr = self.mock_file_attr();
        Ok(ReplyAttr {
            ttl: TTL,
            attr
        })
    }
    
    
    /*
    
    idem

     */

    async fn lookup(
        &self, 
        _req: Request, 
        _parent: &OsStr, 
        _name: &OsStr) -> Result<ReplyEntry> {
        println!("lookup");
        let attr = self.mock_file_attr();
        Ok(ReplyEntry {
            ttl: TTL,
            attr,
        })
    }

    /*
        

        async fn forget(&self, _req: Request, _parent: &OsStr, _nlookup: u64) {}

        

        async fn setattr(
            &self,
            _req: Request,
            path: Option<&OsStr>,
            _fh: Option<u64>,
            set_attr: SetAttr,
        ) -> Result<ReplyAttr> {
            let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            Ok(ReplyAttr {
                ttl: TTL,
                attr: entry.set_attr(set_attr),
            })
        }

        async fn mkdir(
            &self,
            _req: Request,
            parent: &OsStr,
            name: &OsStr,
            mode: u32,
            _umask: u32,
        ) -> Result<ReplyEntry> {
            let path = parent.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if let Entry::Dir(dir) = entry {
                if dir.children.contains_key(name) {
                    return Err(Errno::new_exist());
                }

                let entry = Entry::Dir(Dir {
                    name: name.to_owned(),
                    children: Default::default(),
                    mode: mode as mode_t,
                });
                let attr = entry.attr();

                dir.children.insert(name.to_owned(), entry);

                Ok(ReplyEntry { ttl: TTL, attr })
            } else {
                Err(Errno::new_is_not_dir())
            }
        }

        async fn unlink(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
            let path = parent.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if let Entry::Dir(dir) = entry {
                if dir
                    .children
                    .get(name)
                    .ok_or_else(Errno::new_not_exist)?
                    .is_dir()
                {
                    return Err(Errno::new_is_dir());
                }

                dir.children.remove(name);

                Ok(())
            } else {
                Err(Errno::new_is_not_dir())
            }
        }

        async fn rmdir(&self, _req: Request, parent: &OsStr, name: &OsStr) -> Result<()> {
            let path = parent.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if let Entry::Dir(dir) = entry {
                let child_dir = dir.children.get(name).ok_or_else(Errno::new_not_exist)?;
                if let Entry::Dir(child_dir) = child_dir {
                    if !child_dir.children.is_empty() {
                        return Err(Errno::from(libc::ENOTEMPTY));
                    }
                } else {
                    return Err(Errno::new_is_not_dir());
                }

                dir.children.remove(name);

                Ok(())
            } else {
                Err(Errno::new_is_not_dir())
            }
        }


        async fn rename(
            &self,
            _req: Request,
            origin_parent: &OsStr,
            origin_name: &OsStr,
            parent: &OsStr,
            name: &OsStr,
        ) -> Result<()> {
            let origin_parent = origin_parent.to_string_lossy();
            let origin_parent_paths = split_path(&origin_parent);

            let inner_fs = &mut *self.0.write().await;
            let mut origin_parent_entry = &inner_fs.root;

            for path in &origin_parent_paths {
                if let Entry::Dir(dir) = origin_parent_entry {
                    origin_parent_entry = dir
                        .children
                        .get(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if origin_parent_entry.is_file() {
                return Err(Errno::new_is_not_dir());
            }

            let mut parent_entry = &inner_fs.root;

            let parent = parent.to_string_lossy();
            let parent_paths = split_path(&parent);

            for path in &parent_paths {
                if let Entry::Dir(dir) = parent_entry {
                    parent_entry = dir
                        .children
                        .get(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if parent_entry.is_file() {
                return Err(Errno::new_is_not_dir());
            }

            let mut origin_parent_entry = &mut inner_fs.root;

            for path in origin_parent_paths {
                if let Entry::Dir(dir) = origin_parent_entry {
                    origin_parent_entry = dir.children.get_mut(OsStr::new(path)).unwrap();
                } else {
                    unreachable!()
                }
            }

            let entry = if let Entry::Dir(dir) = origin_parent_entry {
                dir.children
                    .remove(origin_name)
                    .ok_or_else(Errno::new_not_exist)?
            } else {
                return Err(Errno::new_is_not_dir());
            };

            let mut parent_entry = &mut inner_fs.root;

            for path in parent_paths {
                if let Entry::Dir(dir) = parent_entry {
                    parent_entry = dir.children.get_mut(OsStr::new(path)).unwrap();
                } else {
                    unreachable!()
                }
            }

            if let Entry::Dir(dir) = parent_entry {
                dir.children.insert(name.to_owned(), entry);
            } else {
                unreachable!()
            }

            Ok(())
        }

        async fn open(&self, _req: Request, path: &OsStr, flags: u32) -> Result<ReplyOpen> {
            let path = path.to_string_lossy();
            let paths = split_path(&path);


            let mut entry = &self.0.read().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if entry.is_dir() {
                Err(Errno::new_is_dir())
            } else {
                Ok(ReplyOpen { fh: 0, flags })
            }
        }

        async fn read(
            &self,
            _req: Request,
            path: Option<&OsStr>,
            _fh: u64,
            offset: u64,
            size: u32,
        ) -> Result<ReplyData> {
            let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
            let paths = split_path(&path);


            let mut entry = &self.0.read().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            let file = if let Entry::File(file) = entry {
                file
            } else {
                return Err(Errno::new_is_dir());
            };

            let mut cursor = Cursor::new(&file.content);
            cursor.set_position(offset);

            let size = cursor.remaining().min(size as _);

            let mut data = BytesMut::with_capacity(size);
            // safety
            unsafe {
                data.set_len(size);
            }

            cursor.read_exact(&mut data).unwrap();

            Ok(ReplyData { data: data.into() })
        }


        async fn write(
            &self,
            _req: Request,
            path: Option<&OsStr>,
            _fh: u64,
            offset: u64,
            data: &[u8],
            _write_flags: u32,
            _flags: u32,
        ) -> Result<ReplyWrite> {
            let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
            let paths = split_path(&path);


            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            let file = if let Entry::File(file) = entry {
                file
            } else {
                return Err(Errno::new_is_dir());
            };

            let offset = offset as usize;

            if offset < file.content.len() {
                let mut content = &mut file.content.as_mut()[offset..];

                if content.len() >= data.len() {
                    content.write_all(data).unwrap();
                } else {
                    content.write_all(&data[..content.len()]).unwrap();
                    let written = content.len();

                    file.content.put(&data[written..]);
                }
            } else {
                file.content.resize(offset, 0);
                file.content.put(data);
            }

            Ok(ReplyWrite {
                written: data.len() as _,
            })
        }

        async fn release(
            &self,
            _req: Request,
            _path: Option<&OsStr>,
            _fh: u64,
            _flags: u32,
            _lock_owner: u64,
            _flush: bool,
        ) -> Result<()> {
            Ok(())
        }

        async fn fsync(
            &self,
            _req: Request,
            _path: Option<&OsStr>,
            _fh: u64,
            _datasync: bool,
        ) -> Result<()> {
            Ok(())
        }

        async fn flush(
            &self,
            _req: Request,
            _path: Option<&OsStr>,
            _fh: u64,
            _lock_owner: u64,
        ) -> Result<()> {
            Ok(())
        }

        async fn access(&self, _req: Request, _path: &OsStr, _mask: u32) -> Result<()> {
            Ok(())
        }

        async fn create(
            &self,
            _req: Request,
            parent: &OsStr,
            name: &OsStr,
            mode: u32,
            flags: u32,
        ) -> Result<ReplyCreated> {
            let path = parent.to_string_lossy();
            let paths = split_path(&path);


            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            if let Entry::Dir(dir) = entry {
                if dir.children.contains_key(name) {
                    return Err(Errno::new_exist());
                }

                let entry = Entry::File(File {
                    name: name.to_owned(),
                    content: Default::default(),
                    mode: mode as mode_t,
                });
                let attr = entry.attr();

                dir.children.insert(name.to_owned(), entry);

                Ok(ReplyCreated {
                    ttl: TTL,
                    attr,
                    generation: 0,
                    fh: 0,
                    flags,
                })
            } else {
                Err(Errno::new_is_not_dir())
            }
        }

        async fn batch_forget(&self, _req: Request, _paths: &[&OsStr]) {}

        // Not supported by fusefs(5) as of FreeBSD 13.0
        #[cfg(target_os = "linux")]
        async fn fallocate(
            &self,
            _req: Request,
            path: Option<&OsStr>,
            _fh: u64,
            offset: u64,
            length: u64,
            mode: u32,
        ) -> Result<()> {
            use std::os::raw::c_int;

            let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &mut self.0.write().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get_mut(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            let file = if let Entry::File(file) = entry {
                file
            } else {
                return Err(Errno::new_is_dir());
            };

            let offset = offset as usize;
            let length = length as usize;

            match mode as c_int {
                0 => {
                    if offset + length > file.content.len() {
                        file.content.resize(offset + length, 0);
                    }

                    Ok(())
                }

                libc::FALLOC_FL_KEEP_SIZE => {
                    if offset + length > file.content.len() {
                        file.content.reserve(offset + length - file.content.len());
                    }

                    Ok(())
                }

                _ => Err(Errno::from(libc::EOPNOTSUPP)),
            }
        }

        async fn rename2(
            &self,
            req: Request,
            origin_parent: &OsStr,
            origin_name: &OsStr,
            parent: &OsStr,
            name: &OsStr,
            _flags: u32,
        ) -> Result<()> {
            self.rename(req, origin_parent, origin_name, parent, name)
                .await
        }

        async fn lseek(
            &self,
            _req: Request,
            path: Option<&OsStr>,
            _fh: u64,
            offset: u64,
            whence: u32,
        ) -> Result<ReplyLSeek> {
            let path = path.ok_or_else(Errno::new_not_exist)?.to_string_lossy();
            let paths = split_path(&path);

            let mut entry = &self.0.read().await.root;

            for path in paths {
                if let Entry::Dir(dir) = entry {
                    entry = dir
                        .children
                        .get(OsStr::new(path))
                        .ok_or_else(Errno::new_not_exist)?;
                } else {
                    return Err(Errno::new_is_not_dir());
                }
            }

            let file = if let Entry::File(file) = entry {
                file
            } else {
                return Err(Errno::new_is_dir());
            };

            let whence = whence as i32;

            let offset = if whence == libc::SEEK_CUR || whence == libc::SEEK_SET {
                offset
            } else if whence == libc::SEEK_END {
                let size = file.content.len();

                if size >= offset as _ {
                    size as u64 - offset
                } else {
                    0
                }
            } else {
                return Err(libc::EINVAL.into());
            };

            Ok(ReplyLSeek { offset })
        }



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
            let data = self
                .read(req, from_path, fh_in, offset_in, length as _)
                .await?;

            // write_flags set to 0 because we don't care it in this example implement
            let ReplyWrite { written } = self
                .write(req, to_path, fh_out, offset_out, &data.data, 0, flags as _)
                .await?;

            Ok(ReplyCopyFileRange {
                copied: u64::from(written),
            })
        }
        */
}


fn split_path(path: &str) -> Vec<&str> {
    if path == "/" {
        vec![]
    } else {
        path.split(SEPARATOR).skip(1).collect()
    }
}