use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use tracing::{Level, instrument};

use crate::models::*;
use crate::storage::FileSystem;

const APP_V1_BASE_URL: &str = "/api/v1";

pub struct FS(RwLock<FileSystem>);

impl FS {
    pub fn new(fs: FileSystem) -> Self {
        Self(RwLock::new(fs))
    }

    pub fn read(&self) -> RwLockReadGuard<FileSystem> {
        self.0.read().expect("FileSystem read lock poisoned")
    }

    pub fn write(&self) -> RwLockWriteGuard<FileSystem> {
        self.0.write().expect("FileSystem write lock poisoned")
    }
}

// This function configures all routes for your module
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope(APP_V1_BASE_URL)
            .service(list_path)
            .service(get_file_content)
            .service(write_file)
            .service(make_directory)
            .service(delete_item)
            .service(rename),
    );
}

#[get("/list/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn list_path(fs: web::Data<FS>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.read().find(&path) {
        let item = node.read();
        if let Some(children_nodes) = item.get_children() {
            let children: Vec<SerializableFSItem> = children_nodes
                .iter()
                .map(|child| SerializableFSItem::new(child))
                .collect();
            HttpResponse::Ok().json(children)
        } else {
            HttpResponse::BadRequest().body("Path isn't a directory")
        }
    } else {
        HttpResponse::NotFound().body("Path not found")
    }
}

#[get("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_file_content(fs: web::Data<FS>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    match fs.read().read_file(&path, 0) {
        Ok(content) => HttpResponse::Ok().json(SerializableFileContent::new(&content)),
        Err(e) => HttpResponse::InternalServerError().json(format!("Failed to read file: {}", e)),
    }
}

#[put("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn write_file(
    fs: web::Data<FS>,
    path: web::Path<String>,
    json: web::Json<WriteFileRequest>,
) -> impl Responder {
    let path = path.into_inner();
    let offset = json.offset();
    let Ok(data) = json.data() else {
        return HttpResponse::BadRequest().body("Invalid base64 data");
    };

    return match fs.write().write_file(&path, &data, offset) {
        Ok(_) => HttpResponse::Ok().body("Write successful"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Write failed: {}", e)),
    };
}

#[post("/mkdir/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn make_directory(fs: web::Data<FS>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some((parent, name)) = path.rsplit_once('/') {
        let parent_path = if parent.is_empty() { "/" } else { parent };
        match fs.write().make_dir(parent_path, name) {
            Ok(_) => HttpResponse::Ok().body("Directory created"),
            Err(e) => HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e)),
        }
    } else {
        HttpResponse::BadRequest().body("Invalid path")
    }
}

#[delete("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn delete_item(fs: web::Data<FS>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.write().delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(format!("Delete failed: {}", s)),
    }
}

#[put("/rename")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn rename(fs: web::Data<FS>, json: web::Json<RenameRequest>) -> impl Responder {
    let fs = fs.write(); // write lock, modificheremo la struttura

    let old_path = json.old_path();
    let new_path = json.new_path();

    match fs.move_node(&old_path, &new_path) {
        Ok(()) => HttpResponse::Ok().body("Successful renaming!"),
        Err(_) => HttpResponse::BadRequest().body("Something went wrong"),
    }
}
