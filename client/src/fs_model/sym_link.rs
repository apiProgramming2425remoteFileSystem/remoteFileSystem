use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;

use super::Attributes;
use crate::cache::CacheableItem;
#[derive(Clone)]
pub struct SymLink {
    pub name: OsString,
    pub attributes: Option<Attributes>,
    pub target: Option<String>,
}

impl SymLink {
    pub fn new(name: OsString, attributes: Option<Attributes>, target: Option<String>) -> Self {
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

impl CacheableItem for SymLink {
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
