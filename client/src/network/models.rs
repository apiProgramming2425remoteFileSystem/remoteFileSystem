use crate::cache::CacheItem;
use crate::error::FsModelError;
use crate::fs_model::{Attributes, RenameFlags, SetAttr};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

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
    pub attributes: Attributes,
}

impl TryFrom<&CacheItem> for SerializableFSItem {
    type Error = FsModelError;

    fn try_from(item: &CacheItem) -> Result<Self, Self::Error> {
        match item {
            CacheItem::Directory(d) => {
                let attrs = d.attributes.ok_or(FsModelError::ConversionFailed(
                    "Missing attributes for directory".to_string(),
                ))?;

                Ok(SerializableFSItem {
                    name: d.name.to_string_lossy().into_owned(),
                    item_type: ItemType::Directory,
                    attributes: attrs,
                })
            }

            CacheItem::SymLink(l) => {
                let attrs = l.attributes.ok_or(FsModelError::ConversionFailed(
                    "Missing attributes for symlink".to_string(),
                ))?;

                Ok(SerializableFSItem {
                    name: l.name.to_string_lossy().into_owned(),
                    item_type: ItemType::SymLink,
                    attributes: attrs,
                })
            }

            CacheItem::File(f) => {
                let attrs = f.attributes.ok_or(FsModelError::ConversionFailed(
                    "Missing attributes for file".to_string(),
                ))?;

                Ok(SerializableFSItem {
                    name: f.name.to_string_lossy().into_owned(),
                    item_type: ItemType::File,
                    attributes: attrs,
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReadFileRequest {
    offset: usize,
    size: usize,
}

impl ReadFileRequest {
    pub fn new(offset: usize, size: usize) -> Self {
        ReadFileRequest { offset, size }
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
    flags: u32,
}

impl RenameRequest {
    pub fn new(old_path: String, new_path: String, flags: RenameFlags) -> Self {
        RenameRequest {
            old_path,
            new_path,
            flags: flags.bits(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SetAttrRequest {
    pub setattr: SetAttr,
}

impl SetAttrRequest {
    pub fn new(setattr: SetAttr) -> Self {
        Self { setattr }
    }
}

#[derive(Serialize)]
pub struct WriteSymlink {
    target: String,
}

impl WriteSymlink {
    pub fn new(target: &str) -> Self {
        WriteSymlink {
            target: target.to_string(),
        }
    }
}
/* AUTHENTICATION MANAGEMENT */
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

impl LoginRequest {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

/* XATTRIBUTES MANAGEMENT */
#[derive(Debug, Serialize, Deserialize)]
pub struct Xattributes {
    xattributes: Vec<u8>,
}

impl Xattributes {
    pub fn new(xattributes: &[u8]) -> Self {
        Xattributes {
            xattributes: xattributes.to_vec(),
        }
    }

    pub fn get(&self) -> Vec<u8> {
        self.xattributes.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct ListXattributes {
    pub names: Vec<String>,
}
