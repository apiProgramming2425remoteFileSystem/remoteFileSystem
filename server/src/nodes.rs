use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock, Weak};

pub struct File {
    name: OsString,
    size: usize,
    parent: FSNodeWeak,
}

pub struct Directory {
    name: OsString,
    parent: FSNodeWeak,
    children: HashMap<PathBuf, FSNode>,
}

#[derive(Debug)]
pub enum FSItem {
    File(File),
    Directory(Directory),
}

type FSItemCell = RwLock<FSItem>;

pub type FSNodeWeak = Weak<FSItemCell>;

#[derive(Debug, Clone)]
pub struct FSNode(Arc<FSItemCell>);

impl File {
    pub fn new<S: AsRef<OsStr>>(name: S, size: usize, parent: FSNodeWeak) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            size,
            parent,
        }
    }

    pub fn parent(&self) -> FSNodeWeak {
        self.parent.clone()
    }
}

impl Directory {
    pub fn new<S: AsRef<OsStr>>(name: S, parent: FSNodeWeak) -> Self {
        Self {
            name: name.as_ref().to_owned(),
            parent,
            children: HashMap::new(),
        }
    }

    pub fn parent(&self) -> FSNodeWeak {
        self.parent.clone()
    }

    pub fn get_childrens(&self) -> Vec<FSNode> {
        self.children.iter().map(|(_, n)| n.clone()).collect()
    }

    pub fn add(&mut self, item: FSNode) {
        self.children
            .insert(PathBuf::from(item.clone().read().name()), item);
    }

    pub fn get_children<P: AsRef<Path>>(&self, name: P) -> Option<FSNode> {
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

    pub fn parent(&self) -> FSNodeWeak {
        match self {
            FSItem::File(f) => f.parent(),
            FSItem::Directory(d) => d.parent(),
        }
    }

    pub fn get_childrens(&self) -> Option<Vec<FSNode>> {
        match self {
            FSItem::Directory(d) => Some(d.get_childrens()),
            _ => None,
        }
    }

    // can be called only if you are sure that self is a directory
    pub fn add(&mut self, item: FSNode) {
        match self {
            FSItem::Directory(d) => d.add(item),
            _ => panic!("Cannot add item to non-directory"),
        }
    }

    pub fn get_children<P: AsRef<Path>>(&self, name: P) -> Option<FSNode> {
        match self {
            FSItem::Directory(d) => d.get_children(name.as_ref()),
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

    // return the absolute path of the item (of the parent)
    pub fn abs_path(&self) -> PathBuf {
        let mut parts = vec![];
        let mut current = self.parent().upgrade();

        while let Some(node) = current {
            let name = node.read().unwrap().name().to_string();
            parts.insert(0, name);
            current = node.read().unwrap().parent().upgrade();
        }

        if parts.len() < 2 {
            return PathBuf::from("/");
        }

        parts.iter().collect()
    }
}

impl FSNode {
    pub fn new(item: FSItem) -> Self {
        Self(Arc::new(RwLock::new(item)))
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, FSItem> {
        self.0.read().expect("FSNode read lock poisoned")
    }

    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, FSItem> {
        self.0.write().expect("FSNode write lock poisoned")
    }

    pub fn next<P: AsRef<Path>>(&self, name: P) -> Option<FSNode> {
        let path = name.as_ref();
        let next_node = if path == Component::CurDir.as_os_str() {
            self.clone()
        } else if path == Component::ParentDir.as_os_str() {
            FSNode::try_from(&self.read().parent()).ok()?
        } else {
            self.read().get_children(name)?
        };

        Some(next_node)
    }
}

impl TryFrom<&FSNodeWeak> for FSNode {
    type Error = (); // TODO: define proper error type

    fn try_from(value: &FSNodeWeak) -> Result<Self, Self::Error> {
        match value.upgrade() {
            Some(arc) => Ok(FSNode(arc)),
            None => Err(()),
        }
    }
}

impl From<&FSNode> for FSNodeWeak {
    fn from(node: &FSNode) -> Self {
        Arc::downgrade(&node.0)
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
