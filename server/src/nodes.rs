use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::path::{Path, PathBuf};

use crate::models::FileAttr;

#[derive(Clone)]
pub struct File {
    name: OsString,
    attributes: FileAttr,
}

#[derive(Clone)]
pub struct Directory {
    name: OsString,
    children: HashMap<PathBuf, FSItem>,
    attributes: FileAttr,
}

#[derive(Debug, Clone)]
pub enum FSItem {
    File(File),
    Directory(Directory),
}

impl File {
    pub fn new<S: AsRef<OsStr>>(name: S, attributes: FileAttr) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            attributes,
        }
    }
}

impl Directory {
    pub fn new<S: AsRef<OsStr>>(name: S, attributes: FileAttr) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            children: HashMap::new(),
            attributes,
        }
    }

    pub fn get_children(&self) -> Vec<FSItem> {
        self.children.iter().map(|(_, n)| n.clone()).collect()
    }

    pub fn add(&mut self, item: FSItem) {
        self.children.insert(PathBuf::from(item.name()), item);
    }

    pub fn get_child<P: AsRef<Path>>(&self, name: P) -> Option<FSItem> {
        self.children.get(&name.as_ref().to_path_buf()).cloned()
    }

    pub fn remove<P: AsRef<Path>>(&mut self, name: P) {
        self.children.remove(&name.as_ref().to_path_buf());
    }
}

impl FSItem {
    // These methods allow us to use an FSItem in a uniform way
    // regardless of its actual type.
    pub fn name(&self) -> &str {
        let name = match self {
            FSItem::File(f) => &f.name,
            FSItem::Directory(d) => &d.name,
        };
        name.to_str().unwrap()
    }

    pub fn attributes(&self) -> FileAttr {
        match self {
            FSItem::File(f) => f.attributes.clone(),
            FSItem::Directory(d) => d.attributes.clone(),
        }
    }

    pub fn get_children(&self) -> Option<Vec<FSItem>> {
        match self {
            FSItem::Directory(d) => Some(d.get_children()),
            _ => None,
        }
    }

    // can be called only if you are sure that self is a directory
    pub fn add(&mut self, item: FSItem) {
        match self {
            FSItem::Directory(d) => d.add(item),
            _ => panic!("Cannot add item to non-directory"),
        }
    }

    pub fn get_child<P: AsRef<Path>>(&self, name: P) -> Option<FSItem> {
        match self {
            FSItem::Directory(d) => d.get_child(name.as_ref()),
            _ => None,
        }
    }

    pub fn remove<P: AsRef<Path>>(&mut self, name: P) {
        match self {
            FSItem::Directory(d) => d.remove(name.as_ref()),
            _ => panic!("Cannot remove item from non-directory"),
        }
    }

    pub fn set_name<S: AsRef<OsStr>>(&mut self, name: S) {
        match self {
            FSItem::File(f) => f.name = name.as_ref().to_owned(),
            FSItem::Directory(d) => d.name = name.as_ref().to_owned(),
        }
    }
}

impl Debug for File {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("File").field("name", &self.name).finish()
    }
}

impl Debug for Directory {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Directory")
            .field("name", &self.name)
            .field("children_count", &self.children.len())
            .finish()
    }
}
