use super::Attributes;

use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use std::vec::Vec;
use crate::cache::CacheableItem;

#[derive(Clone)]
pub struct Directory {
    pub name: OsString,
    pub attributes: Option<Attributes>,
    pub children: Option<Vec<OsString>>,
}

impl Directory {
    pub fn new(
        name: OsString,
        attributes: Option<Attributes>,
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

impl CacheableItem for Directory {
    fn rename(&mut self, name: OsString) {
        self.name = name;
    }

    fn get_attributes(&self) -> Option<Attributes> {
        self.attributes
    }

    fn invalidate_attributes(&mut self) {
        self.attributes = None;
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
