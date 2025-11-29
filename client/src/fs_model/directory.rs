use std::ffi::OsString;
use crate::fs_model::{File, FileAttr};
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct Directory {
    pub name: OsString,
    pub attributes: Option<FileAttr>,
    pub children: Option<Vec<OsString>>,
}

impl Directory {
    pub fn new(name: OsString, attributes: Option<FileAttr>, children: Option<Vec<OsString>>) -> Self {
        Directory{name, attributes, children }
    }

    pub fn add_child(&mut self, child: OsString) {
        if let Some(children) = &mut self.children {
            children.push(child);
        }
    }

    pub fn merge(&mut self, other: Directory) {
        if other.attributes.is_some() {
            self.attributes = other.attributes;
        }
        if other.children.is_some() {
            self.children = other.children;
        }
    }
}