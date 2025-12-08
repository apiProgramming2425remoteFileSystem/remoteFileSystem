use std::fs;
use std::path::Path;

use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use tracing;
use tracing_actix_web::TracingLogger;

pub mod config;
pub mod error;
pub mod logging;
pub mod models;
pub mod nodes;
pub mod routes;
mod storage;

use error::ServerError;
use storage::FileSystem;


type Result<T> = std::result::Result<T, ServerError>;

/*
fn create_file_system_with_structure() -> FileSystem {
    let mut fs = FileSystem::new(".", false);

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
*/

pub async fn run_server<H: AsRef<str>, F: AsRef<Path>>(
    host: H,
    port: u16,
    fs_root: F,
) -> Result<()> {
    let host = host.as_ref();
    let fs_root = fs_root.as_ref();

    // Create root filesystem directory if it doesn't exist
    if !fs_root.exists() {
        tracing::info!(
            "Filesystem root directory {:?} does not exist. Creating it.",
            fs_root
        );
        fs::create_dir_all(&fs_root).map_err(|err| ServerError::Other(err.into()))?;
    }

    tracing::info!("Starting server at {}:{}", host, port);

    let fs = web::Data::new(FileSystem::new(fs_root));

    HttpServer::new(move || {
        App::new()
            .app_data(fs.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .wrap(TracingLogger::default()) // Middleware for request tracing
            .wrap(Logger::default()) // actix built-in logger
            .configure(routes::configure)
    })
    .bind((host, port))
    .map_err(|err| ServerError::Other(err.into()))?
    .run()
    .await
    .map_err(|err| ServerError::Other(err.into()))
}
