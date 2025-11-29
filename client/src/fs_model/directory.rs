use std::ffi::OsString;
use crate::fs_model::FileAttr;
use std::vec::Vec;

#[derive(Clone, Debug)]
pub struct Directory {
    pub name: OsString,
    pub attributes: FileAttr,
    pub children: Vec<OsString>,
    pub valid_children: bool,
}

impl Directory {
    pub fn new(name: OsString, attributes: FileAttr, children: Vec<OsString>) -> Self {
        Directory{name, attributes, children, valid_children: true}
    }
}