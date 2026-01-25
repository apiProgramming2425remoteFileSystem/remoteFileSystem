use crate::error::{FsModelError, FuseError, NetworkError};
use crate::fs_model;
use crate::fs_model::attributes::Timestamp;
use crate::fs_model::{Attributes, FileType, SetAttr};
use crate::fuse::Fs;
use crate::network::models::{ItemType, SerializableFSItem};
use std::cell::Cell;
use std::ffi::OsString;
use std::ffi::{OsStr, c_void};
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::instrument;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_EXECUTE, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
};
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF,
};
use windows_sys::Win32::Foundation::STATUS_FILE_IS_A_DIRECTORY;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
};
use winfsp::FspError;
use winfsp::filesystem::VolumeInfo;
use winfsp::filesystem::{DirInfo, DirMarker, FileSecurity, ModificationDescriptor, WideNameInfo};
use winfsp::{
    Result, U16CStr,
    filesystem::{FileInfo, FileSystemContext, OpenFileInfo},
    host::FileSystemHost,
};

const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000000C;
const HELLO_DATA: &[u8] = b"Hello from MockFS!\r\n";

macro_rules! fslog {
    ($name:expr, $($arg:expr),*) => {
        println!(
            "[FS] {:<20} | {}",
            $name,
            format!($($arg),*)
        );
    };
}

fn parse_symlink_target(extra_buffer: Option<&[u8]>) -> Result<String> {
    let buf =
        extra_buffer.ok_or_else(|| FsModelError::InvalidInput("Missing reparse buffer".into()))?;

    if buf.len() < 20 {
        return Err(FspError::IO(ErrorKind::InvalidInput));
    }

    let tag = u32::from_le_bytes(buf[0..4].try_into().unwrap());
    if tag != IO_REPARSE_TAG_SYMLINK {
        return Err(FspError::IO(ErrorKind::InvalidInput));
    }

    let print_name_offset = u16::from_le_bytes(buf[12..14].try_into().unwrap()) as usize;
    let print_name_length = u16::from_le_bytes(buf[14..16].try_into().unwrap()) as usize;

    let path_buffer_start = 20;
    let start = path_buffer_start + print_name_offset;
    let end = start + print_name_length;

    if end > buf.len() {
        return Err(FspError::IO(ErrorKind::InvalidInput));
    }

    let u16_slice: &[u16] = unsafe {
        std::slice::from_raw_parts(
            buf[start..end].as_ptr() as *const u16,
            print_name_length / 2,
        )
    };

    let target = OsString::from_wide(u16_slice).to_string_lossy().to_string();

    Ok(target)
}

fn index_from_path(path: &Path) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

pub fn winfsp_path_to_pathbuf(path: &U16CStr) -> Result<PathBuf> {
    // UTF-16 → String
    let mut s = path
        .to_string()
        .map_err(|_| FspError::IO(ErrorKind::InvalidInput))?;

    s = s.replace('\\', "/");

    if s.is_empty() || s == "/" {
        return Ok(PathBuf::from("/"));
    }

    if !s.starts_with('/') {
        s.insert(0, '/');
    }

    Ok(PathBuf::from(s))
}

fn mask_from_access(granted_access: u32) -> u32 {
    let mut mask = 0;

    // READ → R_OK (4)
    if granted_access & (FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0) != 0 {
        mask |= 4;
    }

    // WRITE → W_OK (2)
    if granted_access
        & (FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0 | FILE_WRITE_ATTRIBUTES.0 | DELETE.0)
        != 0
    {
        mask |= 2;
    }

    // EXECUTE → X_OK (1)
    if granted_access & FILE_EXECUTE.0 != 0 {
        mask |= 1;
    }

    mask
}

const WINDOWS_EPOCH_DIFF: u64 = 11644473600; // seconds

fn unix_to_filetime(ts: Timestamp) -> u64 {
    let secs = ts.sec as u64;
    (secs + WINDOWS_EPOCH_DIFF) * 10_000_000
}

fn filetime_to_unix(t: u64) -> Option<Timestamp> {
    if t == 0 {
        return None;
    }

    // FILETIME → UNIX epoch
    let unix_100ns = t - 116444736000000000;
    let secs = unix_100ns / 10_000_000;
    let nanos = (unix_100ns % 10_000_000) * 100;

    Some(Timestamp {
        sec: secs as i64,
        nsec: nanos as u32,
    })
}

fn fill_file_info(info: &mut FileInfo, attr: &Attributes) {
    info.file_attributes = match attr.kind {
        FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
        _ => FILE_ATTRIBUTE_NORMAL,
    };

    info.file_size = attr.size;
    info.allocation_size = if attr.blocks > 0 && attr.blksize > 0 {
        attr.blocks * attr.blksize as u64
    } else {
        attr.size
    };

    info.creation_time = unix_to_filetime(if attr.crtime.is_zero() {
        attr.ctime
    } else {
        attr.crtime
    });

    info.last_access_time = unix_to_filetime(attr.atime);
    info.last_write_time = unix_to_filetime(attr.mtime);
    info.change_time = unix_to_filetime(attr.ctime);

    info.index_number = 0;
    info.hard_links = 0;
    info.ea_size = 0;
}

type FILE_ACCESS_RIGHTS = u32;
type FILE_FLAGS_AND_ATTRIBUTES = u32;

pub struct Handle {
    path: PathBuf,
    delete_on_close: Cell<bool>,
}

impl FileSystemContext for Fs {
    type FileContext = Handle;

    fn open(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {
        self.rt.block_on(async {
            let path = winfsp_path_to_pathbuf(file_name)?;

            // 1. esistenza + tipo
            let attr = self.fs.get_attributes(&path).await?;
            fslog!("open", "path={:?} create={:?}", path, create_options);

            // 2. tipo coerente
            if attr.kind == FileType::Directory && (create_options & FILE_NON_DIRECTORY_FILE) != 0 {
                tracing::error!("Directory creation failed");
                return Err(FspError::NTSTATUS(STATUS_FILE_IS_A_DIRECTORY));
            }

            let info = file_info.as_mut();
            info.file_attributes = match attr.kind {
                FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
                _ => FILE_ATTRIBUTE_NORMAL,
            };

            info.file_size = attr.size;
            info.allocation_size = attr.size;

            Ok(Handle {
                path,
                delete_on_close: Cell::new(false),
            })
        })
    }

    fn close(&self, context: Self::FileContext) {
        fslog!("close", "path={:?}", context.path);
        self.rt.block_on(async {
            if let Err(e) = self.fs.flush_write_buffer().await {
                tracing::error!("flush on close failed for {:?}: {}", context.path, e);
            }
        })
    }

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity> {
        self.rt.block_on(async {
            let path = winfsp_path_to_pathbuf(file_name)?;

            let attr = self.fs.get_attributes(&path).await?;

            let attributes = match attr.kind {
                FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
                FileType::Symlink => FILE_ATTRIBUTE_REPARSE_POINT,
                _ => FILE_ATTRIBUTE_NORMAL,
            };

            Ok(FileSecurity {
                reparse: false,
                sz_security_descriptor: 0,
                attributes,
            })
        })
    }

    fn get_file_info(&self, context: &Self::FileContext, file_info: &mut FileInfo) -> Result<()> {
        self.rt.block_on(async {
            let attr = self.fs.get_attributes(&context.path).await?;

            let file_attributes = match attr.kind {
                FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
                _ => FILE_ATTRIBUTE_NORMAL,
            };

            let allocation_size = if attr.blocks > 0 && attr.blksize > 0 {
                attr.blocks * attr.blksize as u64
            } else {
                attr.size
            };

            file_info.file_attributes = file_attributes;
            file_info.reparse_tag = 0;
            file_info.allocation_size = allocation_size;
            file_info.file_size = attr.size;

            file_info.creation_time = unix_to_filetime(if attr.crtime.is_zero() {
                attr.ctime
            } else {
                attr.crtime
            });

            file_info.last_access_time = unix_to_filetime(attr.atime);
            file_info.last_write_time = unix_to_filetime(attr.mtime);
            file_info.change_time = unix_to_filetime(attr.ctime);

            file_info.index_number = index_from_path(&context.path);
            file_info.hard_links = 0;
            file_info.ea_size = 0;

            Ok(())
        })
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker<'_>,
        buffer: &mut [u8],
    ) -> Result<u32> {
        self.rt.block_on(async {
            let path = &context.path;
            let mut items = self.fs.readdir(path).await?;
            items.sort_by(|a, b| a.name.cmp(&b.name));
            fslog!("read_dir", "path={:?}", path);

            let mut cursor = 0u32;
            let mut start = 0usize;

            // --- Marker handling (NAME-based) ---
            if !marker.is_none() {
                if let Some(name) = marker.inner_as_cstr().and_then(|s| s.to_string().ok()) {
                    if let Some(pos) = items.iter().position(|e| e.name == name) {
                        start = pos + 1;
                    }
                }
            }

            // --- Emit entries ---
            for item in items.iter().skip(start) {
                let mut entry: DirInfo<255> = DirInfo::new();
                fill_file_info(entry.file_info_mut(), &item.attributes);
                entry.set_name(&item.name)?;

                if !entry.append_to_buffer(buffer, &mut cursor) {
                    break;
                }
            }

            DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
            Ok(cursor)
        })
    }

    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        _file_attributes: FILE_FLAGS_AND_ATTRIBUTES,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        extra_buffer: Option<&[u8]>,
        extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {
        self.rt.block_on(async {
            let path = winfsp_path_to_pathbuf(file_name)?;
            fslog!(
                "create",
                "path={:?} create_options=0x{:08x} granted_access=0x{:08x}",
                path,
                create_options,
                granted_access
            );

            let is_dir = create_options & FILE_DIRECTORY_FILE != 0;
            let is_file = create_options & FILE_NON_DIRECTORY_FILE != 0;

            let create = create_options & FILE_CREATE != 0;
            let open = create_options & FILE_OPEN != 0;
            let open_if = create_options & FILE_OPEN_IF != 0;

            let info: &mut FileInfo = file_info.as_mut();

            /* ================= SYMLINK ================= */

            if extra_buffer_is_reparse_point {
                let target = parse_symlink_target(extra_buffer)?;
                self.fs.create_symlink(&path, &target).await?;

                info.file_attributes = FILE_ATTRIBUTE_REPARSE_POINT;
                info.reparse_tag = IO_REPARSE_TAG_SYMLINK;
                info.file_size = 0;
                info.allocation_size = 0;

                return Ok(Handle {
                    path,
                    delete_on_close: Cell::new(false),
                });
            }

            /* ================= DIRECTORY ================= */

            if is_dir {
                if create || open_if {
                    self.fs.mkdir(&path).await?;
                } else if open {
                    self.fs.get_attributes(&path).await?;
                }

                info.file_attributes = FILE_ATTRIBUTE_DIRECTORY;
                info.file_size = 0;
                info.allocation_size = 0;

                return Ok(Handle {
                    path,
                    delete_on_close: Cell::new(false),
                });
            }

            /* ================= FILE ================= */

            if is_file {
                if create || open_if {
                    self.fs
                        .create_file(&path, &FileType::RegularFile, 0, &[])
                        .await?;
                } else if open {
                    self.fs.get_attributes(&path).await?;
                }

                info.file_attributes = FILE_ATTRIBUTE_NORMAL;
                info.file_size = 0;
                info.allocation_size = 0;

                return Ok(Handle {
                    path,
                    delete_on_close: Cell::new(false),
                });
            }

            Err(FspError::IO(ErrorKind::InvalidInput))
        })
    }

    fn cleanup(&self, context: &Self::FileContext, file_name: Option<&U16CStr>, _flags: u32) {
        let _ = self.rt.block_on(async {
            self.fs.flush_write_buffer().await.ok();

            if let Some(name) = file_name {
                if let Ok(path) = winfsp_path_to_pathbuf(name) {
                    fslog!(
                        "cleanup",
                        "path={:?} delete_pending={}",
                        path,
                        context.delete_on_close.get()
                    );
                    self.fs.cache_invalidate(path);
                }
            }

            Ok::<(), ()>(())
        });
        if context.delete_on_close.get() {
            let path = context.path.clone();
            self.rt.block_on(async {
                let _ = self.fs.remove(path).await;
            });
        }
    }

    fn flush(&self, _context: Option<&Self::FileContext>, _file_info: &mut FileInfo) -> Result<()> {
        self.rt.block_on(async {
            self.fs.flush_write_buffer().await?;
            Ok(())
        })
    }

    fn get_security(
        &self,
        _context: &Self::FileContext,
        security_descriptor: Option<&mut [c_void]>,
    ) -> Result<u64> {
        const SD: &[u8] = &[
            0x01, 0x00, 0x04, 0x80, 0x14, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        if let Some(buf) = security_descriptor {
            let dst = unsafe {
                std::slice::from_raw_parts_mut(
                    buf.as_mut_ptr() as *mut u8,
                    buf.len() * std::mem::size_of::<c_void>(),
                )
            };

            let len = SD.len().min(dst.len());
            dst[..len].copy_from_slice(&SD[..len]);
        }

        Ok(SD.len() as u64)
    }

    fn set_security(
        &self,
        _context: &Self::FileContext,
        _security_information: u32,
        _modification_descriptor: ModificationDescriptor,
    ) -> Result<()> {
        Ok(())
    }

    fn overwrite(
        &self,
        context: &Self::FileContext,
        _file_attributes: FILE_FLAGS_AND_ATTRIBUTES,
        _replace_file_attributes: bool,
        allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        self.rt.block_on(async {
            self.fs
                .set_attributes(
                    0,
                    0,
                    &context.path,
                    SetAttr {
                        size: Some(0),
                        ..Default::default()
                    },
                )
                .await
        })?;

        file_info.file_size = 0;
        file_info.allocation_size = allocation_size;

        Ok(())
    }

    fn rename(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> Result<()> {
        let old_path = context.path.clone();
        let new_path = winfsp_path_to_pathbuf(new_file_name)?;
        fslog!(
            "rename",
            "from={:?} to={:?} replace={}",
            old_path,
            new_path,
            replace_if_exists
        );

        if !replace_if_exists {
            match self
                .rt
                .block_on(async { self.fs.get_attributes(&new_path).await })
            {
                Ok(_) => {
                    return Err(FspError::NTSTATUS(0xC0000035u32 as i32));
                }
                Err(_) => {
                    // OK, proceed
                }
            }
        }

        self.rt
            .block_on(async { self.fs.rename(&old_path, &new_path).await })?;

        Ok(())
    }

    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        _file_attributes: u32,
        creation_time: u64,
        last_access_time: u64,
        last_write_time: u64,
        last_change_time: u64,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        let mut setattr = SetAttr::default();

        // timestamps
        setattr.atime = filetime_to_unix(last_access_time);
        setattr.mtime = filetime_to_unix(last_write_time);
        setattr.ctime = filetime_to_unix(last_change_time);

        let needs_call =
            setattr.atime.is_some() || setattr.mtime.is_some() || setattr.ctime.is_some();

        fslog!(
            "set_file_info",
            "path={:?} size={:?} attrs={:?} delete={}",
            context.path,
            file_info.file_size,
            file_info.file_attributes,
            context.delete_on_close.get()
        );

        if needs_call {
            let attrs = self
                .rt
                .block_on(async { self.fs.set_attributes(0, 0, &context.path, setattr).await })?;

            *file_info = attrs.into();
        }

        Ok(())
    }

    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> Result<()> {
        fslog!(
            "set_delete",
            "path={:?} delete_file={}",
            context.path,
            delete_file
        );
        context.delete_on_close.set(delete_file);
        Ok(())
    }

    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        self.rt.block_on(async {
            self.fs
                .set_attributes(
                    0,
                    0,
                    &context.path,
                    SetAttr {
                        size: Some(new_size),
                        ..Default::default()
                    },
                )
                .await
        })?;

        file_info.file_size = new_size;
        file_info.allocation_size = new_size;

        Ok(())
    }

    fn read(&self, context: &Self::FileContext, buffer: &mut [u8], offset: u64) -> Result<u32> {
        self.rt.block_on(async {
            let res = self
                .fs
                .read_file(&context.path, offset as usize, buffer.len())
                .await?;
            let len = res.len();
            buffer[..len].copy_from_slice(&res);
            Ok(len as u32)
        })
    }

    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> Result<u32> {
        self.rt.block_on(async {
            let flags = fs_model::Flags::default();
            let written = self
                .fs
                .write_file(&context.path, &flags, offset as usize, buffer)
                .await?;
            file_info.file_size = file_info.file_size.max(offset + written as u64);
            file_info.allocation_size = file_info.file_size;
            Ok(written as u32)
        })
    }

    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        out_dir_info: &mut DirInfo,
    ) -> Result<()> {
        let name = winfsp_path_to_pathbuf(file_name)?;
        let path = context.path.join(&name);
        let attrs = self
            .rt
            .block_on(async { self.fs.get_attributes(&path).await })?;
        let file_info = out_dir_info.file_info_mut();
        *file_info = attrs.into();
        out_dir_info.set_name(&name)?;
        Ok(())
    }

    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> Result<()> {
        out_volume_info.total_size = 1024 * 1024 * 1024;
        out_volume_info.free_size = 1024 * 1024 * 1024;
        out_volume_info.set_volume_label("remote FS");
        Ok(())
    }

    fn set_volume_label(&self, volume_label: &U16CStr, volume_info: &mut VolumeInfo) -> Result<()> {
        volume_info.set_volume_label("remote FS");
        Ok(())
    }

    fn get_stream_info(&self, context: &Self::FileContext, buffer: &mut [u8]) -> Result<u32> {
        Ok(0)
    }

    fn get_reparse_point_by_name(
        &self,
        file_name: &U16CStr,
        is_directory: bool,
        buffer: &mut [u8],
    ) -> Result<u64> {
        Err(FspError::NTSTATUS(0xC0000275u32 as i32))
    }

    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &mut [u8],
    ) -> Result<u64> {
        Err(FspError::NTSTATUS(0xC0000275u32 as i32))
    }

    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &[u8],
    ) -> Result<()> {
        Err(FspError::NTSTATUS(0xC0000275u32 as i32))
    }

    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &[u8],
    ) -> Result<()> {
        Err(FspError::NTSTATUS(0xC0000275u32 as i32))
    }

    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
    ) -> Result<u32> {
        Ok(0)
    }

    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        file_info: &mut FileInfo,
    ) -> Result<()> {
        Err(FspError::NTSTATUS(0xC00000BBu32 as i32))
    }

    fn control(
        &self,
        context: &Self::FileContext,
        control_code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<u32> {
        Err(FspError::NTSTATUS(0xC0000010u32 as i32))
    }

    fn dispatcher_stopped(&self, normally: bool) {
        if normally {
            println!("Filesystem stopped normally");
        } else {
            println!("Filesystem stopped abnormally");
        }
    }
}

impl From<FsModelError> for FspError {
    fn from(value: FsModelError) -> Self {
        tracing::error!("{}", value);

        match value {
            FsModelError::NotFound(_) => FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0),

            FsModelError::PermissionDenied(_) => FspError::NTSTATUS(STATUS_ACCESS_DENIED.0),

            FsModelError::InvalidInput(_) | FsModelError::ConversionFailed(_) => {
                FspError::NTSTATUS(STATUS_INVALID_PARAMETER.0)
            }

            FsModelError::FileHandlerError => FspError::NTSTATUS(STATUS_INVALID_HANDLE.0),

            FsModelError::WriterError => FspError::NTSTATUS(STATUS_IO_DEVICE_ERROR.0),

            FsModelError::NoData(_) => FspError::NTSTATUS(STATUS_NO_EAS_ON_FILE.0),

            FsModelError::ServerError(net) => net.into(),

            FsModelError::Other(_) => FspError::NTSTATUS(STATUS_UNEXPECTED_IO_ERROR.0),
        }
    }
}

impl From<NetworkError> for FspError {
    fn from(value: NetworkError) -> Self {
        match value {
            NetworkError::ConnectionFailed(_) => FspError::NTSTATUS(STATUS_HOST_UNREACHABLE.0),

            NetworkError::InvalidInput(_) => FspError::NTSTATUS(STATUS_INVALID_PARAMETER.0),

            NetworkError::Request(_) => FspError::NTSTATUS(STATUS_IO_DEVICE_ERROR.0),

            NetworkError::ServerError(FuseError::NotFound(_)) => {
                FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0)
            }

            NetworkError::ServerError(_) => FspError::NTSTATUS(STATUS_UNEXPECTED_IO_ERROR.0),

            NetworkError::UnexpectedResponse(_) => FspError::NTSTATUS(STATUS_BAD_NETWORK_PATH.0),

            NetworkError::InvalidCredentials => FspError::NTSTATUS(STATUS_ACCESS_DENIED.0),

            NetworkError::Other(_) => FspError::NTSTATUS(STATUS_IO_DEVICE_ERROR.0),
        }
    }
}

impl From<Attributes> for FileInfo {
    fn from(attr: Attributes) -> Self {
        let mut info = FileInfo::default();

        info.file_size = attr.size;
        info.allocation_size = attr.blocks * attr.blksize as u64;

        info.creation_time = unix_to_filetime(attr.crtime);
        info.last_access_time = unix_to_filetime(attr.atime);
        info.last_write_time = unix_to_filetime(attr.mtime);
        info.change_time = unix_to_filetime(attr.ctime);

        info.file_attributes = match attr.kind {
            FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
            FileType::Symlink => FILE_ATTRIBUTE_REPARSE_POINT,
            _ => FILE_ATTRIBUTE_NORMAL,
        };

        info
    }
}
