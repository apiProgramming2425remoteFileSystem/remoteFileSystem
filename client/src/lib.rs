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
use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use std::ops::Deref;

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

#[get("/list/{path}")]
async fn list_path(fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.find_full(&path, None) {
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
