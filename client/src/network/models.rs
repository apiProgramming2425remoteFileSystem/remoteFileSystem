use crate::fs_model::attributes::SetAttr;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use crate::cache::CacheItem;
use crate::fs_model::FileAttr;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Directory,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SerializableFSItem {
    pub name: String,
    pub item_type: ItemType,
    pub attributes: FileAttr
}

impl From<&CacheItem> for SerializableFSItem {
    fn from(item: &CacheItem) -> Self {
        match item {
            CacheItem::Directory(d) => SerializableFSItem {
                name: d.name.to_string_lossy().into_owned(),
                item_type: ItemType::Directory,
                attributes: d.attributes,
            },
            CacheItem::File(f) => SerializableFSItem {
                name: f.name.to_string_lossy().into_owned(),
                item_type: ItemType::File,
                attributes: f.attributes,
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
