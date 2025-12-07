use crate::fs_model::attributes::SetAttr;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use crate::cache::CacheItem;
use crate::error::FsModelError;
use crate::fs_model::FileAttr;
use crate::fuse::Fs;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    SymLink,
    Directory,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SerializableFSItem {
    pub name: String,
    pub item_type: ItemType,
    pub attributes: FileAttr
}

impl TryFrom<&CacheItem> for SerializableFSItem {
    type Error = FsModelError;

    fn try_from(item: &CacheItem) -> Result<Self, Self::Error> {
        match item {
            CacheItem::Directory(d) => {
                let attrs = d.attributes.ok_or(FsModelError::ConversionFailed)?;

                Ok(SerializableFSItem {
                    name: d.name.to_string_lossy().into_owned(),
                    item_type: ItemType::Directory,
                    attributes: attrs,
                })
            }

            CacheItem::SymLink(l) => {
                let attrs = l.attributes.ok_or(FsModelError::ConversionFailed)?;

                Ok(SerializableFSItem{
                    name: l.name.to_string_lossy().into_owned(),
                    item_type: ItemType::SymLink,
                    attributes: attrs,
                })
            }

            CacheItem::File(f) => {
                let attrs = f.attributes.ok_or(FsModelError::ConversionFailed)?;

                Ok(SerializableFSItem {
                    name: f.name.to_string_lossy().into_owned(),
                    item_type: ItemType::File,
                    attributes: attrs,
                })
            }
        }
    }
}



#[derive(Debug, Deserialize)]
pub struct ReadFile {
    data: String,
}

impl ReadFile {
    pub fn data(&self) -> Result<Vec<u8>, base64::DecodeError> {
        STANDARD.decode(&self.data)
    }
}

#[derive(Debug, Serialize)]
pub struct WriteFile {
    offset: usize,
    data: String,
}

impl WriteFile {
    pub fn new(offset: usize, data: &[u8]) -> Self {
        Self {
            offset,
            data: STANDARD.encode(data), // encode data in base64 as string
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RenameRequest {
    old_path: String,
    new_path: String,
}

impl RenameRequest {
    pub fn new(old_path: String, new_path: String) -> Self {
        RenameRequest { old_path, new_path }
    }
}

#[derive(Debug, Serialize)]
pub struct SetAttrRequest {
    pub uid: u32,
    pub gid: u32,
    pub setattr: SetAttr,
}

impl SetAttrRequest {
    pub fn new(uid: u32, gid: u32, setattr: SetAttr) -> Self {
        Self {
            uid: uid,
            gid: gid,
            setattr: setattr,
        }
    }
}

#[derive(Serialize)]
pub struct Writelink {
    target: String,
}

impl Writelink{
    pub fn new(target: &str) -> Self {
        Writelink { target: target.to_string() }
    }
}