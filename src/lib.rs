/*pub mod fuse;
pub mod network;
pub mod cache;*/
pub mod fs_model;
/*pub mod daemon;
pub mod config;
pub mod logging;
pub mod util;
pub mod error;*/
use crate::fs_model::node::{FSItem, FSNode, FileSystem};
use actix_web::{HttpResponse, Responder, delete, get, put, web};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, sync::RwLock};

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Dir,
}

#[derive(Serialize)]
pub struct SerializableFSItem {
    name: String,
    item_type: ItemType,
}

#[derive(Serialize)]
pub struct SerializableFileContent {
    content: Vec<u8>,
}

fn serialize_node(node: &FSNode) -> SerializableFSItem {
    let item = node.read().unwrap();
    let item_type = match item.deref() {
        FSItem::File(_) => ItemType::File,
        FSItem::Directory(_) => ItemType::Dir,
    };
    SerializableFSItem {
        name: item.name().to_string(),
        item_type,
    }
}

fn serialize_content(content: Vec<u8>) -> SerializableFileContent {
    SerializableFileContent { content: content }
}

#[get("/list/{path}")]
async fn list_path(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.read().unwrap().find_full(&path, None) {
        let item = node.read().unwrap();
        if let FSItem::Directory(dir) = item.deref() {
            let children: Vec<SerializableFSItem> = dir
                .children
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

#[derive(Deserialize)]
struct WriteFileRequest {
    offset: usize,
    data: String, // accept base64-encoded data as string
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

#[delete("/files/{path}")]
async fn delete_item(fs: web::Data<RwLock<FileSystem>>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.write().unwrap().delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(s),
    }
}
