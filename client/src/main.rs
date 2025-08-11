use std::sync::RwLock;

use actix_web::{App, HttpServer, web};
use client::fs_model::node::FileSystem;
use client::{delete_item, get_file_content, list_path, make_directory, write_file};

fn create_file_system_with_structure() -> FileSystem {
    let mut fs = FileSystem::new();
    fs.make_dir("/", "home").unwrap();
    fs.change_dir("/home").unwrap();
    fs.make_dir(".", "user").unwrap();
    fs.change_dir("./user").unwrap();
    fs.make_file(".", "file.txt").unwrap();
    fs.make_file(".", "file1.txt").unwrap();
    fs.make_dir("..", "user1").unwrap();
    fs.change_dir("../user1").unwrap();
    fs.make_file(".", "file.txt").unwrap();
    fs
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Server listening at http://127.0.0.1:8080");
    let fs = web::Data::new(RwLock::new(create_file_system_with_structure()));
    HttpServer::new(move || {
        App::new()
            .app_data(fs.clone())
            .service(list_path)
            .service(get_file_content)
            .service(write_file)
            .service(make_directory)
            .service(delete_item)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
