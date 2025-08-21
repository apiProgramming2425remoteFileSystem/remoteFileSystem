use std::ops::Deref;

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

#[derive(Serialize)]
pub struct SerializableFileContent {
    content: Vec<u8>,
}

#[derive(Deserialize)]
pub struct WriteFileRequest {
    pub offset: usize,
    pub data: String, // accept base64-encoded data as string
}

pub fn serialize_node(node: &FSNode) -> SerializableFSItem {
    let item = node.read().unwrap();
    let item_type = match item.deref() {
        FSItem::File(_) => ItemType::File,
        FSItem::Directory(_) => ItemType::Directory,
    };
    SerializableFSItem {
        name: item.name().to_string(),
        item_type,
    }
}

pub fn serialize_content(content: Vec<u8>) -> SerializableFileContent {
    SerializableFileContent { content: content }
}
