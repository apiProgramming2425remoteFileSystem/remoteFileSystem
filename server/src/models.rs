use std::fmt::{Debug, Formatter, Result};

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::attributes::*;
use crate::nodes::FSItem;

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Directory,
    SymLink,
}

#[derive(Serialize)]
pub struct SerializableFSItem {
    name: String,
    item_type: ItemType,
    attributes: FileAttr,
}

impl SerializableFSItem {
    pub fn new(item: &FSItem) -> Self {
        let item_type = match item {
            FSItem::File(_) => ItemType::File,
            FSItem::SymLink(_) => ItemType::SymLink,
            FSItem::Directory(_) => ItemType::Directory,
        };
        Self {
            name: item.name().to_string(),
            item_type,
            attributes: item.attributes(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ReadFileRequest {
    offset: usize,
    size: usize,
}

impl ReadFileRequest {
    pub fn new(offset: usize, size: usize) -> Self {
        ReadFileRequest { offset, size }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

#[derive(Debug, Deserialize)]
pub struct OffsetQuery {
    pub offset: usize,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    old_path: String,
    new_path: String,
    flags: u32,
}

impl RenameRequest {
    pub fn new(old_path: String, new_path: String, flags: u32) -> Self {
        Self {
            old_path,
            new_path,
            flags,
        }
    }
    pub fn new_path(&self) -> String {
        self.new_path.clone()
    }
    pub fn old_path(&self) -> String {
        self.old_path.clone()
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }
}

#[derive(Debug, Deserialize)]
pub struct SetAttrRequest {
    pub setattr: SetAttr,
}

impl SetAttrRequest {
    pub fn setattr(&self) -> SetAttr {
        self.setattr.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct SymlinkRequest {
    pub target: String,
}

/* AUTHENTICATION MANAGEMENT */
#[derive(Debug, FromRow)]
pub struct User {
    pub user_id: u32,
    pub group_id: u32,
    pub username: String,
    pub password: String,
}

#[derive(Debug, FromRow)]
pub struct PartialUser {
    pub user_id: u32,
    pub group_id: u32,
    pub username: String,
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}
impl Debug for LoginBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("LoginBody")
            .field("username", &self.username)
            .field("password", &"********")
            .finish()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    pub user_id: u32,
    pub group_id: u32,
    pub token_id: String,
    pub exp: usize,
}

#[derive(Debug, Serialize)]
pub struct Token {
    token: String,
}

impl Token {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthenticatedUser {
    pub user_id: u32,
    pub group_id: u32,
    pub token_id: String,
    pub expiration_time: i64,
}

impl Debug for AuthenticatedUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("AuthenticatedUser")
            .field("user_id", &self.user_id)
            .field("group_id", &self.group_id)
            .field("expiration_time", &self.expiration_time)
            .field("token_id", &"********")
            .finish()
    }
}

/* XATTRIBUTES MANAGEMENT */
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Xattributes {
    xattributes: Vec<u8>,
}

impl Xattributes {
    pub fn get(&self) -> &[u8] {
        self.xattributes.as_slice()
    }
}

#[derive(Debug, Serialize, FromRow, Default)]
pub struct ListXattributes {
    pub names: Vec<String>,
}
