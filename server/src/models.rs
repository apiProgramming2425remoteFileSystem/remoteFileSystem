use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use actix_web::{
    Error, FromRequest, HttpResponse,
    dev::{Payload, ServiceRequest},
    web,
};

use futures::future::{BoxFuture, Ready, err, ok};

use crate::{
    db::{DB, JWT_KEY},
    error::ServerError,
    nodes::FSItem,
};

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
}

impl RenameRequest {
    pub fn new(old_path: String, new_path: String) -> Self {
        Self { old_path, new_path }
    }
    pub fn new_path(&self) -> String {
        self.new_path.clone()
    }
    pub fn old_path(&self) -> String {
        self.old_path.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct SetAttrRequest {
    pub uid: u32,
    pub gid: u32,
    pub setattr: SetAttr,
}

impl SetAttrRequest {
    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub fn gid(&self) -> u32 {
        self.gid
    }

    pub fn setattr(&self) -> SetAttr {
        self.setattr.clone()
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Permission type
pub struct PermissionType {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execution permission
    pub execute: bool,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Permission
pub struct Permission {
    /// Permissions for the file owner.
    pub user: PermissionType,
    /// Permissions for the group owner.
    pub group: PermissionType,
    /// Permissions for all others.
    pub other: PermissionType,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileAttr {
    /// Size in bytes
    pub size: u64,
    /// Size in blocks
    pub blocks: u64,
    /// Time of last access
    pub atime: Timestamp,
    /// Time of last modification
    pub mtime: Timestamp,
    /// Time of last change
    pub ctime: Timestamp,
    pub crtime: Timestamp,
    /// Kind of file (directory, file, pipe, etc)
    pub kind: FileType,
    /// Permissions
    pub perm: Permission,
    /// Number of hard links
    pub nlink: u32,
    /// User id
    pub uid: u32,
    /// Group id
    pub gid: u32,
    /// Rdev
    pub rdev: u32,
    /// block size
    pub blksize: u32,
    /// Flags (macOS only, see chflags(2))
    pub flags: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Timestamp {
    pub sec: i64,
    pub nsec: u32,
}

impl From<SystemTime> for Timestamp {
    fn from(t: SystemTime) -> Self {
        let d = t
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        Timestamp {
            sec: d.as_secs().try_into().unwrap_or(i64::MAX),
            nsec: d.subsec_nanos(),
        }
    }
}

impl From<Timestamp> for SystemTime {
    fn from(t: Timestamp) -> Self {
        let duration = Duration::new(t.sec as u64, t.nsec);
        UNIX_EPOCH.checked_add(duration).unwrap()
    }
}

#[derive(Debug, Copy, Clone, Serialize /*, Ord, PartialOrd, Eq, PartialEq, Hash */)]
pub enum FileType {
    /// Named pipe [`libc::S_IFIFO`]
    NamedPipe,
    /// Character device [`libc::S_IFCHR`]
    CharDevice,
    /// Block device [`libc::S_IFBLK`]
    BlockDevice,
    /// Directory [`libc::S_IFDIR`]
    Directory,
    /// Regular file [`libc::S_IFREG`]
    RegularFile,
    /// Symbolic link [`libc::S_IFLNK`]
    Symlink,
    /// Unix domain socket [`libc::S_IFSOCK`]
    Socket,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SetAttr {
    /// set file or directory mode.
    pub mode: Option<Permission>,
    /// set file or directory uid.
    pub uid: Option<u32>,
    /// set file or directory gid.
    pub gid: Option<u32>,
    /// set file or directory size.
    pub size: Option<u64>,
    /// the lock_owner argument.
    pub lock_owner: Option<u64>,
    /// set file or directory atime.
    pub atime: Option<Timestamp>,
    /// set file or directory mtime.
    pub mtime: Option<Timestamp>,
    /// set file or directory ctime.
    pub ctime: Option<Timestamp>,
    // macOS only attributes
    pub crtime: Option<Timestamp>,
    pub chgtime: Option<Timestamp>,
    pub bkuptime: Option<Timestamp>,
    pub flags: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub bsize: u32,
    pub namelen: u32,
    pub frsize: u32,
}



#[derive(Debug, Deserialize)]
pub struct SymlinkRequest {
    pub target: String,
}


trait Conversion<T>: Sized {
    type Error;

    fn from_target(&self) -> T;
    fn try_to_target(value: T) -> Result<Self, Self::Error>;
}

macro_rules! impl_conversion {
    ($source:ty, $target:ty, $cv:ty) => {
        impl From<$source> for $target
        where
            $source: Conversion<$cv>,
        {
            fn from(value: $source) -> Self {
                value.from_target() as $target
            }
        }

        impl TryFrom<$target> for $source
        where
            $source: Conversion<$cv>,
        {
            type Error = ();

            fn try_from(value: $target) -> Result<Self, Self::Error> {
                Self::try_to_target(value as $cv)
            }
        }
    };
}

impl_conversion!(FileType, u32, u32);
impl_conversion!(FileType, i32, u32);
impl_conversion!(FileType, u16, u32);

impl_conversion!(PermissionType, i32, i32);

impl_conversion!(Permission, u16, i32);

// Implement Conversion trait for FileType using numeric tags.
impl Conversion<u32> for FileType {
    type Error = ();

    fn from_target(&self) -> u32 {
        match self {
            FileType::NamedPipe => 0,
            FileType::CharDevice => 1,
            FileType::BlockDevice => 2,
            FileType::Directory => 3,
            FileType::RegularFile => 4,
            FileType::Symlink => 5,
            FileType::Socket => 6,
        }
    }

    fn try_to_target(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FileType::NamedPipe),
            1 => Ok(FileType::CharDevice),
            2 => Ok(FileType::BlockDevice),
            3 => Ok(FileType::Directory),
            4 => Ok(FileType::RegularFile),
            5 => Ok(FileType::Symlink),
            6 => Ok(FileType::Socket),
            _ => Err(()),
        }
    }
}

// Permissions are represented as 3-bit masks: read=0b100, write=0b010, execute=0b001
impl Conversion<i32> for PermissionType {
    type Error = ();

    fn from_target(&self) -> i32 {
        let mut v = 0;
        if self.read {
            v |= 0b100;
        }
        if self.write {
            v |= 0b010;
        }
        if self.execute {
            v |= 0b001;
        }
        v
    }

    fn try_to_target(value: i32) -> Result<Self, Self::Error> {
        Ok(PermissionType {
            read: (value & 0b100) != 0,
            write: (value & 0b010) != 0,
            execute: (value & 0b001) != 0,
        })
    }
}

// Full Permission packs user/group/other each as 3 bits into a single i32:
// user << 6 | group << 3 | other
impl Conversion<i32> for Permission {
    type Error = ();

    fn from_target(&self) -> i32 {
        let user = self.user.from_target();
        let group = self.group.from_target();
        let other = self.other.from_target();
        ((user << 6) | (group << 3) | other) as i32
    }

    fn try_to_target(value: i32) -> Result<Self, Self::Error> {
        let user_mask = (value >> 6) & 0b111;
        let group_mask = (value >> 3) & 0b111;
        let other_mask = value & 0b111;

        let user = PermissionType::try_to_target(user_mask)?;
        let group = PermissionType::try_to_target(group_mask)?;
        let other = PermissionType::try_to_target(other_mask)?;

        Ok(Permission { user, group, other })
    }
}

/* AUTHENTICATION MANAGEMENT */
#[derive(Debug, FromRow)]
pub struct User {
    pub user_id: u64,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    pub user_id: u64,
    pub token_id: String,
    pub exp: usize, // expiration time
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthenticatedUser {
    pub user_id: i64,
    pub token_id: String,
    pub expiration_time: i64,
}

/*
impl FromRequest for AuthenticatedUser {
    type Error = actix_web::Error;
    type Future = BoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _payload: &mut Payload) -> Self::Future {
        // 1. retrieve Authorization header
        let auth_header = match req.headers().get("Authorization") {
            Some(header) => header,
            None => {
                return Box::pin(async {
                    Err(actix_web::error::ErrorUnauthorized(
                        "Authorization Header is missing.",
                    ))
                });
            }
        };

        // 2. token extraction
        let auth_value = match auth_header.to_str() {
            Ok(s) => s,
            Err(_) => {
                return Box::pin(async {
                    Err(actix_web::error::ErrorUnauthorized("Header is not valid"))
                });
            }
        };

        if !auth_value.starts_with("Bearer ") {
            return Box::pin(async {
                Err(actix_web::error::ErrorUnauthorized(
                    "Token format is not valid.",
                ))
            });
        }
        let token_string = &auth_value[7..];

        // 3. Validation and decode key configuration
        let decoding_key = DecodingKey::from_secret(JWT_KEY);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        // 4. Token decoding and verification
        let token_data = match decode::<Claims>(token_string, &decoding_key, &validation) {
            Ok(data) => data,
            Err(_) => {
                return Box::pin(async {
                    Err(actix_web::error::ErrorUnauthorized("Token is invalid."))
                });
            }
        };

        let user_id = token_data.claims.user_id as i64;
        let token_id = token_data.claims.token_id;
        let expiration_time = token_data.claims.exp as i64;

        // 5. Check if token is expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind.")
            .as_secs() as i64;
        let is_expired = now >= expiration_time;

        if is_expired {
            return Box::pin(async {
                Err(actix_web::error::ErrorUnauthorized("Token is expired."))
            });
        }

        // 6. Check if the token has been revoked
        let pool_opt = req.app_data::<web::Data<DB>>().cloned();

        Box::pin(async move {
            let pool = match pool_opt {
                Some(p) => p,
                None => {
                    return Err(actix_web::error::ErrorInternalServerError(
                        "Error during the retrieval of database connection.",
                    ));
                }
            };

            let is_revoked = match pool.is_token_revoked(user_id, &token_id).await {
                Ok(flag) => flag,
                Err(_) => {
                    return Err(actix_web::error::ErrorInternalServerError(
                        "Error while checking token revocation.",
                    ));
                }
            };

            if is_revoked {
                return Err(actix_web::error::ErrorUnauthorized(
                    "Token has been revoked.",
                ));
            }

            Ok(AuthenticatedUser {
                user_id,
                token_id,
                expiration_time,
            })
        })
    }
}
*/

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

#[derive(Debug, Serialize, FromRow)]
pub struct ListXattributes {
    pub names: Vec<String>,
}
