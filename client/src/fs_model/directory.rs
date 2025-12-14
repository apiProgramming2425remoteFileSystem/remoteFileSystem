use crate::fs_model::{File, FileAttr};
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::vec::Vec;

#[derive(Clone)]
pub struct Directory {
    pub name: OsString,
    pub attributes: Option<FileAttr>,
    pub children: Option<Vec<OsString>>,
}

impl Directory {
    pub fn new(
        name: OsString,
        attributes: Option<FileAttr>,
        children: Option<Vec<OsString>>,
    ) -> Self {
        Directory {
            name,
            attributes,
            children,
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

impl Debug for Directory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::from("attributes: ");
        if self.attributes.is_some() {
            result += "present  ";
        } else {
            result += "missing  ";
        }
        result += "children: [";
        if let Some(children) = &self.children {
            for child in children {
                result += &format!(" {:?} ", child);
            }
        }
        result += "]";
        write!(f, "{}", result)
    }
}
