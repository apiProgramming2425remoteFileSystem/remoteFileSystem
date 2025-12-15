use crate::fs_model::{File, FileAttr};
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Clone)]
pub struct SymLink {
    pub name: OsString,
    pub attributes: Option<FileAttr>,
    pub target: Option<String>,
}

impl SymLink {
    pub fn new(name: OsString, attributes: Option<FileAttr>, target: Option<String>) -> Self {
        SymLink {
            name,
            attributes,
            target,
        }
    }

    pub fn merge(&mut self, other: SymLink) {
        if other.attributes.is_some() {
            self.attributes = other.attributes;
        }
        if other.target.is_some() {
            self.target = other.target;
        }
    }
}

impl Debug for SymLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::from("attributes: ");
        if self.attributes.is_some() {
            result += "present  ";
        } else {
            result += "missing  ";
        }
        result += "target: ";
        if let Some(target) = &self.target {
            result += &format!("{:?}", target);
        } else {
            result += "missing";
        }
        write!(f, "{}", result)
    }
}
