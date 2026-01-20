use std::path::Path;

use anyhow::{Result, anyhow};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, sleep};

use server::config::RfsConfig;
use server::config::logging::{LogFormat, LogLevel, LogTargets};
use server::db::DB;
use server::logging::Logging;
use server::run_server;

const DB_PATH: &str = "database/test-db.sqlite";

/// Helper function to bootstrap the application in a background task.
pub async fn start_server_app(
    mut config: RfsConfig,
) -> Result<(Logging, HttpClient, JoinHandle<()>)> {
    // Initialize logging based on config
    let log = Logging::from(&config.logging)?;

    let server_host = config.server_host.clone();
    let filesystem_root = config.filesystem_root.clone();

    let lst =
        tokio::net::TcpListener::bind(format!("{}:{}", &server_host, config.server_port)).await?;
    let local_addr = lst.local_addr()?;
    config.server_port = local_addr.port();
    println!("Test server will start at {}", local_addr);

    let listener = lst.into_std()?;

    // Initialize database connection
    let db_conn = DB::open_connection(DB_PATH).await?;

    // Spawn the core application logic in a separate Tokio task.
    // This allows the test logic to run concurrently in the main thread.
    let app_handle = tokio::spawn(async move {
        // Start the server
        let server = run_server(listener, &filesystem_root, db_conn)
            .await
            .expect("Failed to run async server");

        server.await.expect("Server runtime error");
    });

    // Wait for the server to start
    wait_ready(&local_addr.to_string(), Duration::from_secs(3)).await?;

    // Wait a bit for the app to start
    let http_client = HttpClient::new(&format!(
        "http://{}:{}/api/v1",
        &config.server_host, config.server_port
    ));

    Ok((log, http_client, app_handle))
}

async fn wait_ready(address: &str, wait_time: Duration) -> Result<()> {
    let deadline = Instant::now() + wait_time;

    while Instant::now() < deadline {
        if tokio::net::TcpStream::connect(address).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    Err(anyhow!(
        "Server did not start listening on {} within {} seconds",
        address,
        wait_time.as_secs()
    ))
}

pub struct HttpClient {
    base_url: String,
    http_client: ClientWithMiddleware,
}

impl HttpClient {
    pub fn new(base_url: &str) -> Self {
        let reqwest_client = Client::new();
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

        let middleware_client = ClientBuilder::new(reqwest_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        HttpClient {
            base_url: base_url.to_string(),
            http_client: middleware_client,
        }
    }

    pub fn set_url_path<S: AsRef<str>>(&self, api: S, path: S) -> String {
        format!(
            "{}/{}/{}",
            self.base_url,
            api.as_ref(),
            urlencoding::encode(path.as_ref())
        )
    }

    pub fn set_url<S: AsRef<str>>(&self, api: S) -> String {
        format!("{}/{}", self.base_url, api.as_ref())
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        self.http_client.get(url)
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        self.http_client.post(url)
    }

    pub fn put(&self, url: &str) -> RequestBuilder {
        self.http_client.put(url)
    }

    pub fn delete(&self, url: &str) -> RequestBuilder {
        self.http_client.delete(url)
    }
}

pub fn get_config(fs_root: &Path) -> RfsConfig {
    RfsConfig {
        server_host: "localhost".to_string(),
        server_port: 0, // Use port 0 to let the OS assign an available port
        filesystem_root: fs_root.to_path_buf(),
        ..Default::default()
    }
}
