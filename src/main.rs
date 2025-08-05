use project::{fs_model, list_path};
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
    fs.make_link("/home", "link_user", "/home/user").unwrap();
    fs
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut fs = web::Data::new(create_file_system_with_structure());
    println!("Server in ascolto su http://127.0.0.1:8080");
    HttpServer::new(|| {
        App::new()
            .app_data(fs.clone())
            .service(list_path)
        })
        .workers(1) // to remove/change when thread-safe
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
