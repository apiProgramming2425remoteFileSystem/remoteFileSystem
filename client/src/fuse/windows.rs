#![cfg(windows)]

use std::ffi::{c_void, OsStr};
use winfsp::filesystem::{DirInfo, DirMarker, FileSecurity, ModificationDescriptor, WideNameInfo};
use winfsp::filesystem::VolumeInfo;
use winfsp::host::VolumeParams;


use winfsp::{filesystem::{
    FileInfo,
    FileSystemContext,
    OpenFileInfo,
}, host::FileSystemHost, Result, U16CStr};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL,
};


/// Filesystem WinFSP minimale e vuoto
pub struct MockFs;

type FILE_ACCESS_RIGHTS = u32;
type FILE_FLAGS_AND_ATTRIBUTES = u32;


const HELLO_DATA: &[u8] = b"Hello from MockFS!\r\n";

impl MockFs {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Clone)]
enum NodeKind {
    RootDir,
    HelloFile,
}

pub struct Handle {
    kind: NodeKind,
}

impl FileSystemContext for MockFs {
    type FileContext = Handle;

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {

        let name = file_name.to_string_lossy();
        println!("open {}", name);

        match name.as_ref() {
            "\\" => {
                let info = file_info.as_mut();
                info.file_attributes = FILE_ATTRIBUTE_DIRECTORY;
                info.file_size = 0;
                info.allocation_size = 0;

                Ok(Handle { kind: NodeKind::RootDir })
            }

            "\\hello.txt" => {
                let data_len = HELLO_DATA.len() as u64;
                let info = file_info.as_mut();
                info.file_attributes = FILE_ATTRIBUTE_NORMAL;
                info.file_size = data_len;
                info.allocation_size = data_len;

                Ok(Handle { kind: NodeKind::HelloFile })
            }

            _ => Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound)),
        }
    }


    fn close(&self, _context: Self::FileContext) {
        println!("Calling close");
        // niente
    }

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        security_descriptor: Option<&mut [c_void]>,
        reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity>{
        println!("Calling get security by name");
        Ok(FileSecurity{reparse: false, sz_security_descriptor: 0,  attributes: FILE_ATTRIBUTE_DIRECTORY})
    }

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<()> {

        match context.kind {
            NodeKind::RootDir => {
                file_info.file_attributes = FILE_ATTRIBUTE_DIRECTORY;
                file_info.file_size = 0;
                file_info.allocation_size = 0;
            }

            NodeKind::HelloFile => {
                let len = HELLO_DATA.len() as u64;
                file_info.file_attributes = FILE_ATTRIBUTE_NORMAL;
                file_info.file_size = len;
                file_info.allocation_size = len;
            }
        }

        Ok(())
    }


    fn read_directory(
        &self,
        _context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker<'_>,
        buffer: &mut [u8],
    ) -> Result<u32> {

        let mut cursor = 0u32;

        // Solo alla prima chiamata
        if marker.is_none(){
            let mut entry: DirInfo<255> = DirInfo::new();

            let info = entry.file_info_mut();
            info.file_attributes = FILE_ATTRIBUTE_NORMAL;
            info.file_size = HELLO_DATA.len() as u64;
            info.allocation_size = HELLO_DATA.len() as u64;

            entry.set_name("hello.txt")?;

            if !entry.append_to_buffer(buffer, &mut cursor) {
                return Ok(cursor);
            }

            DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
        }

        Ok(cursor)
    }






    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        file_attributes: FILE_FLAGS_AND_ATTRIBUTES,
        security_descriptor: Option<&[c_void]>,
        allocation_size: u64,
        extra_buffer: Option<&[u8]>,
        extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext>{
        println!("Calling create");
        Ok(Handle{kind: NodeKind::RootDir})
    }

    fn cleanup(
        &self,
        context: &Self::FileContext,
        file_name: Option<&U16CStr>,
        flags: u32,
    ){
        println!("Calling cleanup");

    }

    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        file_info: &mut FileInfo,
    ) -> Result<()>{
        println!("Calling flush");
        Ok(())
    }


    fn get_security(
        &self,
        context: &Self::FileContext,
        security_descriptor: Option<&mut [c_void]>,
    ) -> Result<u64>{
        println!("Calling get security");
        Ok(0)
    }

    fn set_security(
        &self,
        context: &Self::FileContext,
        security_information: u32,
        modification_descriptor: ModificationDescriptor,
    ) -> Result<()>{
        println!("Calling set security");
        Ok(())
    }

    fn overwrite(
        &self,
        context: &Self::FileContext,
        file_attributes: FILE_FLAGS_AND_ATTRIBUTES,
        replace_file_attributes: bool,
        allocation_size: u64,
        extra_buffer: Option<&[u8]>,
        file_info: &mut FileInfo,
    ) -> Result<()>{
        println!("Calling overwrite");
        Ok(())
    }

    fn rename(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> Result<()>{
        println!("Calling rename");
        Ok(())
    }

    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        file_attributes: u32,
        creation_time: u64,
        last_access_time: u64,
        last_write_time: u64,
        last_change_time: u64,
        file_info: &mut FileInfo,
    ) -> Result<()>{
        println!("Calling set basic info");
        Ok(())
    }

    fn set_delete(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        delete_file: bool,
    ) -> Result<()>{
        println!("Calling set delete");
        Ok(())
    }


    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> Result<()>{
        println!("Calling set file size");
        Ok(())
    }


    fn read(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<u32> {

        if !matches!(context.kind, NodeKind::HelloFile) {
            return Ok(0);
        }

        let offset = offset as usize;
        if offset >= HELLO_DATA.len() {
            return Ok(0);
        }

        let available = &HELLO_DATA[offset..];
        let to_copy = available.len().min(buffer.len());

        buffer[..to_copy].copy_from_slice(&available[..to_copy]);
        Ok(to_copy as u32)
    }


    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> Result<u32>{
        println!("Calling write");
        Ok(0)
    }

    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        out_dir_info: &mut DirInfo,
    ) -> Result<()>{
        println!("Calling get dir info by name");
        Ok(())
    }
    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> Result<()>{
        println!("Calling get volume info");
        out_volume_info.total_size = 1024 * 1024 * 1024;
        out_volume_info.free_size = 1024 * 1024 * 1024;
        out_volume_info.set_volume_label("MockFS");
        Ok(())
    }

    fn set_volume_label(
        &self,
        volume_label: &U16CStr,
        volume_info: &mut VolumeInfo,
    ) -> Result<()>{
        println!("Calling set volume label");
        Ok(())
    }

    fn get_stream_info(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
    ) -> Result<u32>{
        println!("Calling get stream info");
        Ok(0)
    }

    fn get_reparse_point_by_name(
        &self,
        file_name: &U16CStr,
        is_directory: bool,
        buffer: &mut [u8],
    ) -> Result<u64>{
        println!("Calling get reparse point by name");
        Ok(0)
    }
    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &mut [u8],
    ) -> Result<u64>{
        println!("Calling get reparse point");
        Ok(0)
    }

    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &[u8],
    ) -> Result<()>{
        println!("Calling set reparse point");
        Ok(())
    }

    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &U16CStr,
        buffer: &[u8],
    ) -> Result<()>{
        println!("Calling delete reparse point");
        Ok(())
    }

    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
    ) -> Result<u32>{
        println!("Calling get extended attributes");
        Ok(0)
    }

    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        file_info: &mut FileInfo,
    ) -> Result<()>{
        println!("Calling set extended attributes");
        Ok(())
    }

    fn control(
        &self,
        context: &Self::FileContext,
        control_code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<u32>{
        println!("Calling control");
        Ok(0)
    }

    fn dispatcher_stopped(&self, normally: bool){
        println!("Calling dispatcher_stopped");
    }

}


pub fn mount_mock_fs() -> Result<()> {
    let fs = MockFs::new();

    let mut params = VolumeParams::default();


    let mut host = FileSystemHost::new(params, fs)?;

    host.start()?;

    println!("[WinFSP] mounting filesystem on X:");

    host.mount("X:")?;

    println!("[WinFSP] mounted successfully");

    println!("[WinFSP] mounted on X:, press Ctrl+C to exit");

    // blocca il thread
    std::thread::park();

    Ok(())
}
