use std::ops::Deref;
use std::sync::RwLock;

use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use tracing::{Level, instrument};

use crate::models::*;
use crate::storage::{FSItem, FileSystem};

const APP_V1_BASE_URL: &str = "/api/v1";

// This function configures all routes for your module
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope(APP_V1_BASE_URL)
            .service(list_path)
            .service(get_file_content)
            .service(write_file)
            .service(make_directory)
            .service(delete_item),
    );
}

#[get("/list/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn list_path(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.read().unwrap().find(&path) {
        let item = node.read().unwrap();
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
async fn get_file_content(
    fs: web::Data<RwLock<FileSystem>>,
    path: web::Path<String>,
) -> impl Responder {
    let path = path.into_inner();

    match fs.read().unwrap().read_file(&path, 0) {
        Ok(content) => HttpResponse::Ok().json(SerializableFileContent::new(&content)),
        Err(e) => HttpResponse::InternalServerError().json(e),
    }
}

#[put("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn write_file(
    fs: web::Data<RwLock<FileSystem>>,
    path: web::Path<String>,
    json: web::Json<WriteFileRequest>,
) -> impl Responder {
    let path = path.into_inner();
    let offset = json.offset();
    let Ok(data) = json.data() else {
        return HttpResponse::BadRequest().body("Invalid base64 data");
    };

    return match fs.write().unwrap().write_file(&path, &data, offset) {
        Ok(_) => HttpResponse::Ok().body("Write successful"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Write failed: {}", e)),
    };
}

#[post("/mkdir/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn make_directory(
    fs: web::Data<RwLock<FileSystem>>,
    path: web::Path<String>,
) -> impl Responder {
    let path = path.into_inner();
    if let Some((parent, name)) = path.rsplit_once('/') {
        let parent_path = if parent.is_empty() { "/" } else { parent };
        match fs.write().unwrap().make_dir(parent_path, name) {
            Ok(_) => HttpResponse::Ok().body("Directory created"),
            Err(e) => HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e)),
        }
    } else {
        HttpResponse::BadRequest().body("Invalid path")
    }
}

#[delete("/files/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn delete_item(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.write().unwrap().delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(s),
    }
}
