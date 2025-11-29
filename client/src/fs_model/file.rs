use std::ffi::OsString;
use crate::fs_model::attributes::FileAttr;
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct File {
    pub name: OsString,
    pub attributes: FileAttr,
    pub content: Vec<u8>,
    pub valid_content: bool,
}

impl File {
    pub fn new(name: OsString, attributes: FileAttr, content: Vec<u8>) -> Self {
        File{name, attributes, content, valid_content: true}
    }
}