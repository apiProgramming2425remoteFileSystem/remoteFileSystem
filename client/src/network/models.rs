use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Directory,
}

#[derive(Debug, Deserialize)]
pub struct SerializableFSItem {
    pub name: String,
    pub item_type: ItemType,
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
