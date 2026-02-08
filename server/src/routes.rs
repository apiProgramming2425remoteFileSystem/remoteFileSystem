use actix_web::http::StatusCode;
use actix_web::middleware::from_fn;
use actix_web::{HttpResponse, Responder, ResponseError, delete, get, post, put, web};
use bytes::Bytes;
use std::path::Path;
use tracing::{Level, instrument};

use crate::api_err;
use crate::attributes::{Operation, OperationQuery};
use crate::db::DB;
use crate::error::ApiError;
use crate::middleware::auth_middleware;
use crate::models::*;
use crate::storage::*;

type Result<T> = std::result::Result<T, ApiError>;

const APP_V1_BASE_URL: &str = "/api/v1";

// Group of all routes for easy management
struct Routes;

impl Routes {
    const AUTH: &'static str = "/auth";
}

// This function configures all routes for your module
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope(APP_V1_BASE_URL)
            // Health check route
            .service(health_check)
            // Authentication routes
            .service(
                web::scope(Routes::AUTH).service(login).service(
                    web::scope("")
                        .wrap(from_fn(auth_middleware))
                        .service(logout),
                ),
            )
            // Filesystem operations routes protected by auth middleware
            .service(
                web::scope("")
                    .wrap(from_fn(auth_middleware))
                    .service(list_path)
                    .service(get_file_content)
                    .service(write_file)
                    .service(make_directory)
                    .service(delete_item)
                    .service(rename)
                    .service(get_attributes)
                    .service(set_attributes)
                    .service(get_permissions)
                    .service(get_stats)
                    .service(create_symlink)
                    .service(read_symlink)
                    .service(set_x_attributes)
                    .service(get_x_attributes)
                    .service(list_x_attributes)
                    .service(delete_x_attributes),
                //.service(resolve_child)
            ),
    )
    .default_service(web::route().to(|req: actix_web::HttpRequest| async move {
        tracing::warn!("MISSING ROUTE: {} {}", req.method(), req.path());
        actix_web::HttpResponse::BadRequest().body("Route not found")
    }));
}

#[get("/list/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn list_path(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Read)?;

    let Some(item) = fs.find(&path) else {
        return Err(api_err!(NotFound, "Path not found"));
    };
    let Some(children_nodes) = item.get_children() else {
        return Err(api_err!(NotADirectory, "Path isn't a directory"));
    };
    let children: Vec<SerializableFSItem> =
        children_nodes.iter().map(SerializableFSItem::new).collect();

    Ok(HttpResponse::Ok().json(children))
}

#[get("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_file_content(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    json: web::Json<ReadFileRequest>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Read)?;

    let offset = json.offset();
    let size = json.size();

    let data = fs.read_file(&path, offset, size)?;

    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .body(data))
}

#[put("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn write_file(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    query: web::Query<OffsetQuery>,
    body: Bytes,
) -> Result<impl Responder> {
    let path = path.into_inner();

    user.check_permission(&fs, &path, Operation::Write)?;

    let offset = query.offset;

    fs.write_file(user.user_id, user.group_id, &path, &body, offset)?;

    let attr = fs.get_attributes(&path)?;

    Ok(HttpResponse::Ok().json(attr))
}

#[post("/mkdir/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn make_directory(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    let (parent, name) = match path.rsplit_once('/') {
        Some((p, n)) => (if p.is_empty() { "/" } else { p }, n),
        None => return Err(api_err!(InvalidInput, "Invalid path")),
    };

    user.check_permission(&fs, parent, Operation::Write)?;

    fs.make_dir(user.user_id, user.group_id, parent, name)?;

    let attributes = fs.get_attributes(path.as_str())?;

    Ok(HttpResponse::Ok().json(attributes))
}

#[delete("/files/{path}")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn delete_item(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    pool: web::Data<DB>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Write)?;

    fs.delete(path.as_str())?;
    pool.remove_all_x_attributes(&path).await?;

    Ok(HttpResponse::Ok().body("Successful deletion!"))
}

#[get("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    let attributes = fs.get_attributes(path.as_str())?;

    Ok(HttpResponse::Ok().json(attributes))
}

#[put("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn set_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    json: web::Json<SetAttrRequest>,
) -> Result<impl Responder> {
    let path = path.into_inner();

    let new_attributes = json.setattr();

    let attributes =
        fs.set_attributes(path.as_str(), user.user_id, user.group_id, new_attributes)?;

    Ok(HttpResponse::Ok().json(attributes))
}

#[get("/permissions/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_permissions(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    query: web::Query<OperationQuery>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    let query = query.into_inner();

    let mask = query.get_mask()?;
    if mask & 4 != 0 {
        user.check_permission(&fs, &path, Operation::Read)?;
    }
    if mask & 2 != 0 {
        user.check_permission(&fs, &path, Operation::Write)?;
    }
    if mask & 1 != 0 {
        user.check_permission(&fs, &path, Operation::Execute)?;
    }

    Ok(HttpResponse::Ok().finish())
}

#[get("/stats/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_stats(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Read)?;
    let stats = fs.get_fs_stats(path.as_str())?;

    Ok(HttpResponse::Ok().json(stats))
}

#[put("/rename")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn rename(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    json: web::Json<RenameRequest>,
    pool: web::Data<DB>,
) -> Result<impl Responder> {
    let old_path = json.old_path();
    let new_path = json.new_path();
    let flags = RenameFlags::from_bits_truncate(json.flags());
    user.check_permission(&fs, &old_path, Operation::Write)?;
    user.check_permission(&fs, &new_path, Operation::Write)?;

    fs.rename(&old_path, &new_path, flags)?;
    if flags.contains(RenameFlags::EXCHANGE) {
        pool.exchange_x_attributes(&old_path, &new_path).await?;
    } else {
        pool.rename_x_attributes(&old_path, &new_path).await?;
    }

    Ok(HttpResponse::Ok().body("Successful renaming!"))
}

#[post("/symlink/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn create_symlink(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    body: web::Json<SymlinkRequest>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Write)?;

    let target = &body.target;

    let attributes = fs.create_symlink(&path, target)?;

    Ok(HttpResponse::Ok().json(attributes))
}

#[get("/symlink/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn read_symlink(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Read)?;
    let target = fs.read_symlink(path.as_str())?;

    Ok(HttpResponse::Ok().json(target))
}

/* AUTHENTICATION MANAGEMENT */
#[post("/login")]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn login(pool: web::Data<DB>, form: web::Json<LoginBody>) -> Result<impl Responder> {
    let token = pool
        .authenticate_user(&form.username, &form.password)
        .await?;

    match token {
        Some(t) => Ok(HttpResponse::Ok().json(Token::new(t))),
        None => Err(ApiError::Unauthorized(
            "Invalid username or password".into(),
        )),
    }
}

#[post("/logout")]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
pub async fn logout(user: AuthenticatedUser, pool: web::Data<DB>) -> Result<impl Responder> {
    pool.insert_revoked_token(&user).await?;
    Ok(HttpResponse::Ok().body("Logged out"))
}

/* XATTRIBUTES MANEGEMENT */
#[put("/xattributes/{path}/names/{name}")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn set_x_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    pool: web::Data<DB>,
    path_params: web::Path<(String, String)>,
    json: web::Json<Xattributes>,
) -> Result<impl Responder> {
    let (name, path) = path_params.into_inner();
    user.check_permission(&fs, &path, Operation::Write)?;

    pool.set_x_attributes(&path, &name, json.get()).await?;

    Ok(HttpResponse::Ok().finish())
}

#[get("/xattributes/{path}/names/{name}")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn get_x_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    pool: web::Data<DB>,
    path_params: web::Path<(String, String)>,
) -> Result<impl Responder> {
    let (name, path) = path_params.into_inner();

    user.check_permission(&fs, &path, Operation::Read)?;

    let option = pool.get_x_attributes(&path, &name).await?;
    match option {
        Some(attr) => Ok(HttpResponse::Ok().json(attr)),
        None => Ok(HttpResponse::NoContent().finish()),
    }
}

#[get("/xattributes/{path}/names")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn list_x_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    pool: web::Data<DB>,
    path: web::Path<String>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    user.check_permission(&fs, &path, Operation::Read)?;
    let names = pool.list_x_attributes(&path).await?;
    Ok(HttpResponse::Ok().json(names))
}

#[delete("/xattributes/{path}/names/{name}")]
#[instrument(skip(fs, pool), ret(level = Level::DEBUG))]
async fn delete_x_attributes(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    pool: web::Data<DB>,
    path_params: web::Path<(String, String)>,
) -> Result<impl Responder> {
    let (name, path) = path_params.into_inner();
    user.check_permission(&fs, &path, Operation::Write)?;

    pool.remove_x_attributes(&path, &name).await?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/health")]
#[instrument()]
async fn health_check() -> Result<impl Responder> {
    Ok(HttpResponse::Ok().body("RemoteFS Server is running"))
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::AlreadyExists(_) => StatusCode::CONFLICT,
            ApiError::NotADirectory(_) => StatusCode::BAD_REQUEST,
            ApiError::IsADirectory(_) => StatusCode::BAD_REQUEST,
            ApiError::DirectoryNotEmpty(_) => StatusCode::CONFLICT,
            ApiError::PermissionDenied(_) => StatusCode::FORBIDDEN,
            ApiError::OperationNotPermitted(_) => StatusCode::FORBIDDEN,
            ApiError::StorageFull(_) => StatusCode::INSUFFICIENT_STORAGE,
            ApiError::OutOfMemory(_) => StatusCode::INSUFFICIENT_STORAGE,
            ApiError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            ApiError::FileTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            ApiError::CrossDeviceLink(_) => StatusCode::NOT_IMPLEMENTED,
            ApiError::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::TextFileBusy(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::ResourceBusy(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::TryAgain(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(self)
    }
}

impl AuthenticatedUser {
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub fn check_permission(&self, fs: &FileSystem, path: &str, op: Operation) -> Result<()> {
        match fs.is_allowed(self.user_id, self.group_id, Path::new(path), op) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ApiError::PermissionDenied(String::from(
                "You do not have the needed permissions.",
            ))),
            Err(e) => Err(e.into()),
        }
    }
}
