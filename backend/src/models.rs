use std::ops::Deref;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::storage::{FSItem, FSNode};

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Directory,
}

#[derive(Serialize)]
pub struct SerializableFSItem {
    name: String,
    item_type: ItemType,
}

impl SerializableFSItem {
    pub fn new(node: &FSNode) -> Self {
        let item = node.read().unwrap();
        let item_type = match item.deref() {
            FSItem::File(_) => ItemType::File,
            FSItem::Directory(_) => ItemType::Directory,
        };
        Self {
            name: item.name().to_string(),
            item_type,
        }
    }
}

#[derive(Serialize)]
pub struct SerializableFileContent {
    data: String,
}

impl SerializableFileContent {
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: STANDARD.encode(data),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    offset: usize,
    data: String, // accept base64-encoded data as string
}

impl WriteFileRequest {
    pub fn new(offset: usize, data: String) -> Self {
        Self { offset, data }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn data(&self) -> Result<Vec<u8>, base64::DecodeError> {
        STANDARD.decode(&self.data)
    }
}
