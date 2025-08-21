use std::ops::Deref;
use std::sync::RwLock;

use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use base64::{Engine, engine::general_purpose::STANDARD};

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
async fn list_path(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.read().unwrap().find(&path) {
        let item = node.read().unwrap();
        if let Some(children_nodes) = item.get_children() {
            let children: Vec<SerializableFSItem> = children_nodes
                .iter()
                .map(|child| serialize_node(child))
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
async fn get_file_content(
    fs: web::Data<RwLock<FileSystem>>,
    path: web::Path<String>,
) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.read().unwrap().find_full(&path, None) {
        let item = node.read().unwrap();
        if let FSItem::File(f) = item.deref() {
            HttpResponse::Ok().json(serialize_content(f.content.clone()))
        } else {
            HttpResponse::BadRequest().body("Path is not a file.")
        }
    } else {
        HttpResponse::NotFound().body("Path not found")
    }
}

#[put("/files/{path}")]
async fn write_file(
    fs: web::Data<RwLock<FileSystem>>,
    path: web::Path<String>,
    json: web::Json<WriteFileRequest>,
) -> impl Responder {
    let path = path.into_inner();
    let offset = json.offset;

    let decoded_data = match STANDARD.decode(&json.data) {
        Ok(bytes) => bytes,
        Err(_) => return HttpResponse::BadRequest().body("Invalid base64 data"),
    };

    return match fs.read().unwrap().write_file(&path, &decoded_data, offset) {
        Ok(_) => HttpResponse::Ok().body("Write successful"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Write failed: {}", e)),
    };
}

#[post("/mkdir/{path}")]
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
async fn delete_item(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.write().unwrap().delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(s),
    }
}
