use std::ffi::OsString;
use crate::fs_model::attributes::FileAttr;
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct File {
    pub name: OsString,
    pub attributes: Option<FileAttr>,
    pub content: Option<Vec<u8>>,
}

impl File {
    pub fn new(name: OsString, attributes: Option<FileAttr>, content: Option<Vec<u8>>) -> Self {
        File{name, attributes, content}
    }

    pub fn write_content(&mut self, data: &[u8], offset: usize) {
        let end = offset + data.len();
        let content = self.content.get_or_insert_with(|| vec![0u8; end]);
        if content.len() < end {
            content.resize(end, 0);
        }
        content[offset..end].copy_from_slice(data);
    }

    pub fn merge(&mut self, other: File) {
        if other.attributes.is_some() {
            self.attributes = other.attributes;
        }
        if other.content.is_some() {
            self.content = other.content;
        }
    }
}