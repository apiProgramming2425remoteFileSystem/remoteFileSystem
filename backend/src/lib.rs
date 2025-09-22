use std::sync::RwLock;

use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use tracing;
use tracing_actix_web::TracingLogger;

pub mod config;
pub mod error;
pub mod logging;
pub mod models;
pub mod routes;
pub mod storage;

fn create_file_system_with_structure() -> storage::FileSystem {
    let mut fs = storage::FileSystem::new();

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

pub async fn run_server(host: &str, port: u16) -> anyhow::Result<()> {
    tracing::info!("Starting backend server at {}:{}", host, port);
    let fs = web::Data::new(RwLock::new(create_file_system_with_structure()));

    HttpServer::new(move || {
        App::new()
            .app_data(fs.clone())
            .wrap(TracingLogger::default()) // Middleware for request tracing
            .wrap(Logger::default()) // actix built-in logger
            .configure(routes::configure)
    })
    .bind((host, port))?
    .run()
    .await?;

    Ok(())
}
