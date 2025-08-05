/*pub mod fuse;
pub mod network;
pub mod cache;*/
pub mod fs_model;
/*pub mod daemon;
pub mod config;
pub mod logging;
pub mod util;
pub mod error;*/
use std::ops::Deref;
use actix_web::{get, web, HttpResponse, Responder};
use serde::Serialize;
use crate::fs_model::node::{FSItem, FSNode, FileSystem};

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Dir,
    Symlink,
}

#[derive(Serialize)]
pub struct SerializableFSItem {
    name: String,
    item_type: ItemType
}

fn serialize_node(node: &FSNode) -> SerializableFSItem {
    let item = node.borrow();
    let item_type = match item.deref() {
        FSItem::File(_) => ItemType::File,
        FSItem::Directory(_) => ItemType::Dir,
        FSItem::SymLink(_) => ItemType::Symlink,
    };
    SerializableFSItem {
        name: item.name().to_string(),
        item_type,
    }
}


#[get("/list/{path}")]
async fn list_path(fs: web::Data<FileSystem>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    if let Some(node) = fs.find_full(&path, None){
        let item = node.borrow();
        if let FSItem::Directory(dir) = item.deref() {
            let children: Vec<SerializableFSItem> =
                dir.children.iter().map(|child| serialize_node(child)).collect();
            HttpResponse::Ok().json(children)
        }
        else {
            HttpResponse::BadRequest().body("Il path non è una directory")
        }
    }
    else {
        HttpResponse::NotFound().body("Path not found")
    }
}