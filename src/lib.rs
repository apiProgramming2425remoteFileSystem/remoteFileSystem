mod binary;
mod client;
mod server;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::binary::BinaryBuilder;
use crate::client::ClientProcess;
use crate::server::ServerProcess;

pub const WAIT_TIMEOUT_SECS: u64 = 5;
pub const DEFAULT_DB_PATH: &str = "database/remote_fs_db.sqlite";
pub const DEFAULT_USER: &str = "test_user";
pub const DEFAULT_PASS: &str = "test_password";
pub const DEFAULT_UID: &str = "1";
pub const DEFAULT_GID: &str = "1";

/// Holds the running processes.
/// When it is destroyed (Drop), it automatically kills the client and server and unmounts the directory.
#[derive(Debug)]
pub struct Context {
    pub server: Option<ServerProcess>,
    pub client: Option<ClientProcess>,
}

impl Context {
    /// Returns the mount point path if the client is running
    pub fn mount_point(&self) -> Option<PathBuf> {
        let c = self.client.as_ref()?;
        Some(c.mount_point.to_owned())
    }

    /// Returns the server root path if the server is running
    pub fn server_root(&self) -> Option<PathBuf> {
        let s = self.server.as_ref()?;
        Some(s.fs_root.to_owned())
    }

    pub fn stop_server(&mut self) {
        let Some(s) = self.server.take() else {
            return;
        };
        println!("Manually stopping server...");
        drop(s);
    }

    pub fn stop_client(&mut self) {
        let Some(c) = self.client.take() else {
            return;
        };
        println!("Manually stopping client...");
        drop(c);
    }

    pub fn remount_client(&mut self, system_builder: SystemBuilder) -> Result<()> {
        let server_url = if let Some(ref srv) = self.server {
            format!("http://{}:{}", srv.host, srv.port)
        } else {
            return Err(anyhow!("Cannot remount client without a running server"));
        };

        // Start new client
        let mut new_client = ClientProcess::spawn(
            system_builder.client,
            &system_builder.mount_point,
            &server_url,
            &system_builder.log_strategy,
        )?;

        // Wait for readiness
        new_client.wait_ready(Duration::from_secs(WAIT_TIMEOUT_SECS))?;

        // Replace old client
        self.client = Some(new_client);

        Ok(())
    }
}

/// List of logging strategies
#[derive(Clone, Debug)]
pub enum LogStrategy {
    /// No log output -> Stdio::null()
    Silent,
    /// Prints to stdout -> Stdio::inherit()
    Console,
    /// Saves to file -> Stdio::from(File)
    ToFile(PathBuf),
}

/// Builder for the entire test system (server + client)
#[derive(Clone, Debug)]
pub struct SystemBuilder {
    /// Specific configuration for the Server
    pub server: BinaryBuilder,
    /// Specific configuration for the Client
    pub client: BinaryBuilder,
    /// Logging strategy
    log_strategy: LogStrategy,
    /// Host address for the server
    host: String,
    /// Port for the server
    port: u16,
    /// Server root directory
    server_root: PathBuf,
    /// Path to the database file
    db_path: PathBuf,
    /// Whether to initialize the database
    init_db: bool,
    /// Mount point for the client
    mount_point: PathBuf,
}

impl SystemBuilder {
    pub fn new(
        host: &str,
        port: u16,
        server_root: &Path,
        db_path: &Path,
        mount_point: &Path,
    ) -> Self {
        Self {
            server: BinaryBuilder::new(),
            client: BinaryBuilder::new(),
            log_strategy: LogStrategy::Console,
            host: host.to_string(),
            port,
            server_root: PathBuf::from(server_root),
            db_path: PathBuf::from(db_path),
            mount_point: PathBuf::from(mount_point),
            init_db: true,
        }
    }

    /// Run without starting the server
    pub fn no_server(&mut self) -> &mut Self {
        self.server.disable();
        self
    }

    /// Run without starting the client
    pub fn no_client(&mut self) -> &mut Self {
        self.client.disable();
        self
    }

    // Helpers to set logging strategy

    pub fn silent(&mut self) -> &mut Self {
        self.log_strategy = LogStrategy::Silent;
        self
    }

    pub fn console(&mut self) -> &mut Self {
        self.log_strategy = LogStrategy::Console;
        self
    }

    pub fn log_to_file(&mut self, path: &Path) -> &mut Self {
        self.log_strategy = LogStrategy::ToFile(path.to_path_buf());
        self
    }

    /// Sets the server host address
    pub fn host(&mut self, host: &str) -> &mut Self {
        self.host = host.to_string();
        self
    }

    /// Sets the server port
    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port = port;
        self
    }

    /// Sets whether to initialize the database
    pub fn init_db(&mut self, init: bool) -> &mut Self {
        self.init_db = init;
        self
    }

    // Final build and start
    pub fn build(self) -> Result<Context> {
        // Start Server and Client as per configuration
        let server_proc = if self.server.enabled() {
            Some(ServerProcess::spawn(
                self.server, // Consumes the builder
                &self.host,
                self.port,
                &self.server_root,
                &self.db_path,
                &self.log_strategy,
                self.init_db,
            )?)
        } else {
            None
        };

        let mut client_proc = if self.client.enabled() {
            let server_url = if let Some(ref srv) = server_proc {
                format!("http://{}:{}", srv.host, srv.port)
            } else {
                format!("http://{}:{}", self.host, self.port)
            };

            Some(ClientProcess::spawn(
                self.client, // Consumes the builder
                &self.mount_point,
                &server_url,
                &self.log_strategy,
            )?)
        } else {
            None
        };

        let wait_duration = Duration::from_secs(WAIT_TIMEOUT_SECS);

        // Wait for readiness
        thread::scope(|s| {
            if let Some(ref srv) = server_proc {
                s.spawn(|| {
                    srv.wait_ready(wait_duration)
                        .expect("Server failed to become ready");
                });
            }
            if let Some(ref mut cli) = client_proc {
                s.spawn(|| {
                    cli.wait_ready(wait_duration)
                        .expect("Client failed to become ready");
                });
            }
        });

        println!("System setup complete.");

        Ok(Context {
            server: server_proc,
            client: client_proc,
        })
    }
}

impl Default for SystemBuilder {
    // Default start both with default configs
    fn default() -> Self {
        Self {
            server: BinaryBuilder::new(),
            client: BinaryBuilder::new(),
            log_strategy: LogStrategy::Console,
            host: "localhost".to_string(),
            port: 8080,
            server_root: PathBuf::from("/remote-fs"),
            db_path: PathBuf::from(DEFAULT_DB_PATH),
            mount_point: PathBuf::from("/mnt/remote-fs"),
            init_db: true,
        }
    }
}

/// Helper per il logging
// NOTE: Used by both server and client processes, be sure both binaries accept the same args
pub fn apply_logging(cmd: &mut Command, log_strategy: &LogStrategy, binary_name: &str) {
    match log_strategy {
        LogStrategy::Silent => {
            cmd.arg("--log-targets").arg("none");
            // We also silence the process streams to keep test output clean
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        LogStrategy::Console => {
            cmd.arg("--log-targets").arg("console");
            // Inherit allows to show logs
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        LogStrategy::ToFile(path) => {
            // Extract directory for LOG_DIR env var
            let (log_dir, log_file_name) = match path.extension() {
                Some(_) => (
                    path.parent().expect("Invalid log file path"),
                    PathBuf::from(path.file_stem().expect("Invalid log file name")),
                ),
                None => (path.as_path(), PathBuf::from(format!("{binary_name}_log"))),
            };

            if !path.exists() {
                std::fs::create_dir_all(log_dir).expect("Could not create log directory");
            }

            println!("Log directory for {}: {:?}", binary_name, log_dir);
            println!("Log file name for {}: {:?}", binary_name, log_file_name);

            // Pass the CLI arguments to your application
            cmd.arg("--log-targets").arg("file");
            cmd.arg("--log-dir").arg(log_dir);
            cmd.arg("--log-file").arg(log_file_name);
            // cmd.arg("--log-format").arg("pretty");
            // cmd.arg("--log-level").arg("debug");

            // Stdio Management for File Logging, inherit the rest
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
    }
}
