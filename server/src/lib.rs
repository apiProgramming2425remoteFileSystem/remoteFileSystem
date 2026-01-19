use std::fs;
use std::path::Path;

use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use tracing_actix_web::TracingLogger;

pub mod app;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod logging;
pub mod models;
pub mod nodes;
pub mod routes;

mod attributes;
mod middleware;
mod storage;
mod util;

use db::DB;
use error::RfsServerError;
use storage::FileSystem;

type Result<T> = std::result::Result<T, RfsServerError>;

pub async fn run_server<H: AsRef<str>, F: AsRef<Path>>(
    host: H,
    port: u16,
    fs_root: F,
    db: DB,
) -> Result<()> {
    let host = host.as_ref();
    let fs_root = fs_root.as_ref();
    // let pool = DB::open_connection()
    //     .await
    //     .map_err(|err| RfsServerError::Other(err.into()))?;

    // Create root filesystem directory if it doesn't exist
    if !fs_root.exists() {
        tracing::info!(
            "Filesystem root directory {:?} does not exist. Creating it.",
            fs_root
        );
        fs::create_dir_all(fs_root).map_err(|err| {
            RfsServerError::Other(anyhow::format_err!(
                "Could not create root directory: {}",
                err
            ))
        })?;
    }

    tracing::info!("Starting server at {}:{}", host, port);

    let fs = web::Data::new(FileSystem::new(fs_root));
    let db = web::Data::new(db);

    HttpServer::new(move || {
        App::new()
            .app_data(fs.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(db.clone())
            .wrap(TracingLogger::default()) // Middleware for request tracing
            .wrap(Logger::default()) // actix built-in logger
            .configure(routes::configure)
    })
    .bind((host, port))
    .map_err(|err| RfsServerError::Other(anyhow::format_err!("Could not bind server: {}", err)))?
    .run()
    .await
    .map_err(|err| RfsServerError::Other(anyhow::format_err!("Server runtime error: {}", err)))
}
