use actix_web::middleware::from_fn;
use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use bytes::Bytes;
use tracing::{Level, instrument};

use crate::db::DB;
use crate::middleware::auth_middleware;
use crate::models::*;
use crate::storage::*;

const APP_V1_BASE_URL: &str = "/api/v1";

// Group of all routes for easy management
struct Routes;

impl Routes {
    const AUTH: &'static str = "/api/v1/auth";
}

// This function configures all routes for your module
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        // Authentication route
        web::scope(Routes::AUTH).service(login),
    )
    .service(
        web::scope(APP_V1_BASE_URL)
            // Filesystem operations routes protected by auth middleware
            .wrap(from_fn(auth_middleware))
            .service(logout)
            .service(list_path)
            .service(get_file_content)
            .service(write_file)
            .service(make_directory)
            .service(delete_item)
            .service(rename)
            .service(resolve_child)
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
    );
}

#[get("/list/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn list_path(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    let Some(item) = fs.find(&path) else {
        return HttpResponse::NotFound().json(String::from("Path not found"));
    };
    let Some(children_nodes) = item.get_children() else {
        return HttpResponse::BadRequest().json(String::from("Path isn't a directory"));
    };
    let children: Vec<SerializableFSItem> = children_nodes
        .iter()
        .map(|child| SerializableFSItem::new(child))
        .collect();
    HttpResponse::Ok().json(children)
}

#[get("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_file_content(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    json: web::Json<ReadFileRequest>,
) -> impl Responder {
    let path = path.into_inner();
    let offset = json.offset();
    let size = json.size();

    let data = match fs.read_file(&path, offset, size) {
        Ok(content) => content,
        Err(e) => {
            return HttpResponse::InternalServerError().json(format!("Failed to read file: {}", e));
        }
    };

    HttpResponse::Ok()
        .content_type("application/octet-stream")
        .body(data)
}

#[put("/files/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn write_file(
    user: AuthenticatedUser,
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    query: web::Query<OffsetQuery>,
    body: Bytes,
) -> impl Responder {
    let path = path.into_inner();
    let offset = query.offset;

    match fs.write_file(&path, &body, offset) {
        Ok(_) => {
            let attr = fs
                .get_attributes(&path)
                .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;
            Ok(HttpResponse::Ok().json(attr))
        }
        Err(e) => Err(actix_web::error::ErrorInternalServerError(e)),
    }
}

#[post("/mkdir/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn make_directory(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    let (parent, name) = match path.rsplit_once('/') {
        Some((p, n)) => (if p.is_empty() { "/" } else { p }, n),
        None => return HttpResponse::BadRequest().body("Invalid path"),
    };
    if let Err(e) = fs.make_dir(parent, name) {
        tracing::error!("mkdir failed: {}", e);
        return HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e));
    }

    let attributes = match fs.get_attributes(path.as_str()) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("mkdir failed: {}", e);
            return HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e));
        }
    };

    HttpResponse::Ok().json(attributes)
}

#[delete("/files/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn delete_item(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(format!("Delete failed: {}", s)),
    }
}

/* Inutile se lookup = getAttr */
#[get("/resolve/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn resolve_child(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.get_attributes(path.as_str()) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[get("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_attributes(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.get_attributes(path.as_str()) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::NotFound().json(e.to_string());
        }
    }
}

#[put("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn set_attributes(
    user: AuthenticatedUser, 
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    json: web::Json<SetAttrRequest>,
) -> impl Responder {
    let path = path.into_inner();

    let uid = json.uid();
    let gid = json.gid();
    let new_attributes = json.setattr();

    match fs.set_attributes(path.as_str(), uid, gid, new_attributes) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/permissions/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn get_permissions(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    match fs.get_permissions(path.as_str()) {
        Ok(permissions) => HttpResponse::Ok().json(permissions),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/stats/{path}")]
async fn get_stats(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    match fs.get_fs_stats(path.as_str()) {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[put("/rename")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn rename(user: AuthenticatedUser, fs: web::Data<FileSystem>, json: web::Json<RenameRequest>) -> impl Responder {
    let old_path = json.old_path();
    let new_path = json.new_path();

    match fs.rename(&old_path, &new_path) {
        Ok(()) => HttpResponse::Ok().body("Successful renaming!"),
        Err(_) => HttpResponse::BadRequest().body("Something went wrong"),
    }
}

#[post("/symlink/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn create_symlink(
    user: AuthenticatedUser, 
    fs: web::Data<FileSystem>,
    path: web::Path<String>,
    body: web::Json<SymlinkRequest>,
) -> impl Responder {
    let path = path.into_inner();
    let target = &body.target;

    match fs.create_symlink(&path, target) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            HttpResponse::InternalServerError().body(format!("{}", e))
        }
    }
}

#[get("/symlink/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn read_symlink(user: AuthenticatedUser, fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    match fs.read_symlink(path.as_str()) {
        Ok(target) => HttpResponse::Ok().json(target),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            HttpResponse::InternalServerError().json(e.to_string())
        }
    }
}

/* AUTHENTICATION MANAGEMENT */
#[post("/login")]
#[instrument(skip(pool ), ret(level = Level::DEBUG))]
async fn login(pool: web::Data<DB>, form: web::Json<LoginBody>) -> impl Responder {
    let result = pool.authenticate_user(&form.username, &form.password).await;

    match result {
        Ok(token) => match token {
            Some(t) => return HttpResponse::Ok().json(Token::new(t)),
            None => return HttpResponse::Unauthorized().finish(),
        },
        Err(e) => return HttpResponse::InternalServerError().json(e.to_string()),
    };
}

#[post("/logout")]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
pub async fn logout(user: AuthenticatedUser, pool: web::Data<DB>) -> impl Responder {
    match pool.insert_revoked_token(&user).await {
        Ok(_) => return HttpResponse::Ok().body("Logged out"),
        Err(e) => return HttpResponse::InternalServerError().json(e.to_string()),
    };
}

/* XATTRIBUTES MANEGEMENT */
#[put("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn set_x_attributes(
    user: AuthenticatedUser, 
    pool: web::Data<DB>,
    path: web::Path<String>,
    name: web::Path<String>,
    json: web::Json<Xattributes>,
) -> impl Responder {
    let path = path.into_inner();
    let name = name.into_inner();

    match pool.set_x_attributes(&path, &name, &json.get()).await {
        Ok(()) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[get("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn get_x_attributes(
    user: AuthenticatedUser, 
    pool: web::Data<DB>,
    name: web::Path<String>,
    path: web::Path<String>,
) -> impl Responder {
    let path = path.into_inner();
    let name = name.into_inner();

    let result = pool.get_x_attributes(&path, &name).await;
    match result {
        Ok(option) => match option {
            Some(attr) => HttpResponse::Ok().json(attr),
            None => HttpResponse::NotFound().finish(),
        },
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/xattributes/{path}/names")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn list_x_attributes(user: AuthenticatedUser, pool: web::Data<DB>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    let result = pool.list_x_attributes(&path).await;
    match result {
        Ok(option) => match option {
            Some(names) => HttpResponse::Ok().json(names),
            None => HttpResponse::NotFound().finish(),
        },
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[delete("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn delete_x_attributes(
    user: AuthenticatedUser, 
    pool: web::Data<DB>,
    path: web::Path<String>,
    name: web::Path<String>,
) -> impl Responder {
    let path = path.into_inner();
    let name = name.into_inner();

    match pool.remove_x_attributes(&path, &name).await {
        Ok(()) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
    }
}
