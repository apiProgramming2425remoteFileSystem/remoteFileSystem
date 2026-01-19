use std::fs;
use std::net::TcpListener;
use std::path::Path;

use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use tracing_actix_web::TracingLogger;

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

use db::DB;
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

pub async fn run_server<F: AsRef<Path>>(listener: TcpListener, fs_root: F) -> Result<Server> {
    let fs_root = fs_root.as_ref();
    let pool = DB::open_connection()
        .await
        .map_err(|err| ServerError::Other(err.into()))?;

    // Create root filesystem directory if it doesn't exist
    if !fs_root.exists() {
        tracing::info!(
            "Filesystem root directory {:?} does not exist. Creating it.",
            fs_root
        );
        fs::create_dir_all(&fs_root).map_err(|err| {
            ServerError::Other(anyhow::format_err!(
                "Could not create root directory: {}",
                err
            ))
        })?;
    }

    let local_address = listener.local_addr().map_err(|err| {
        ServerError::Other(anyhow::format_err!("Could not get local address: {}", err))
    })?;

    tracing::info!(
        "Starting RemoteFS server at {}:{}",
        local_address.ip(),
        local_address.port()
    );

    let fs = web::Data::new(FileSystem::new(fs_root));
    let db = web::Data::new(pool);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(fs.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(db.clone())
            .wrap(TracingLogger::default()) // Middleware for request tracing
            .wrap(Logger::default()) // actix built-in logger
            .configure(routes::configure)
    })
    .listen(listener)
    .map_err(|err| {
        ServerError::Other(anyhow::format_err!(
            "Could not listen on provided listener: {}",
            err
        ))
    })?
    .run();

    Ok(server)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dummy() {
        assert_eq!(1 + 1, 2);
    }
}
