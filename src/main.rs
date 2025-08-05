use project::{list_path};
use project::fs_model::node::FileSystem;
use actix_web::{App, HttpServer, web};


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
    HttpServer::new(|| {
        App::new()
            .app_data(web::Data::new(create_file_system_with_structure()))
            .service(list_path)
        })
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
