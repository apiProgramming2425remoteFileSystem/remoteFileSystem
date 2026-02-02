use std::path::Path;

use anyhow::{anyhow, Result};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tempfile::TempDir;

use server::config::{LoggingConfig, RfsConfig};
//use server::config::logging::{LogFormat, LogLevel, LogTargets};
use server::db::DB;
use server::logging::Logging;
use server::run_server;
use server::error::LoggingError;
use once_cell::sync::OnceCell;


static LOGGER: OnceCell<Logging> = OnceCell::new();


const TEST_USER: &str = "test_user";
const TEST_PASSWORD: &str = "test_password";
const TEST_USER_ID: i64 = 1;
const TEST_GROUP_ID: i64 = 1;

/// Ottiene un token di test legato al DB specificato
pub async fn get_test_token(db_path: &Path) -> Result<String> {
    let db = DB::open_connection(db_path).await?;

    if !db.user_exists(TEST_USER).await? {
        db.create_user(TEST_USER_ID, TEST_GROUP_ID, TEST_USER, TEST_PASSWORD).await?;
    }

    let token = db
        .authenticate_user(TEST_USER, TEST_PASSWORD)
        .await?
        .expect("token must exist");

    Ok(token)
}

pub fn init_logging(config: &LoggingConfig) -> &Logging {
    LOGGER.get_or_init(|| Logging::from(config).unwrap())
}



/// Bootstrap del server in background, ritorna logging, client HTTP e handle
/// ora ritorna solo il client poi vedremo
pub async fn start_server_app(
    mut config: RfsConfig,
) -> Result<(HttpClient, JoinHandle<()>, TempDir)> {
    // Logging
    let _ = init_logging(&config.logging);


    // Listener TCP
    let lst = tokio::net::TcpListener::bind(format!("{}:{}", &config.server_host, config.server_port)).await?;
    let local_addr = lst.local_addr()?;
    config.server_port = local_addr.port();
    println!("Test server will start at {}", local_addr);
    let listener = lst.into_std()?;

    // DB temporaneo
    let tmp_dir = tempfile::tempdir()?;
    let db_path = tmp_dir.path().join("test-db.sqlite");
    let db_conn = DB::open_connection(&db_path).await?;

    // Avvio server in background
    let filesystem_root = config.filesystem_root.clone();
    let app_handle = tokio::spawn(async move {
        // Start the server
        let server = run_server(listener, &filesystem_root, db_conn)
            .await
            .expect("Failed to run async server");

        server.await.expect("Server runtime error");
    });

    // Attendi che il server sia pronto
    wait_ready(&local_addr.to_string(), Duration::from_secs(3)).await?;

    // Ottieni token di default
    let token = get_test_token(&db_path).await?;

    // Client HTTP con token
    let http_client = HttpClient::new_with_token(
        &format!("http://{}:{}/api/v1", &config.server_host, config.server_port),
        Some(&token),
    );

    Ok((http_client, app_handle, tmp_dir))
}

/// Attende che il server risponda sulla porta
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

/// Client HTTP che può inserire automaticamente il token negli header
pub struct HttpClient {
    base_url: String,
    http_client: ClientWithMiddleware,
}

impl HttpClient {
    pub fn new_with_token(base_url: &str, token: Option<&str>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token).parse().unwrap(),
            );
        }

        let reqwest_client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let middleware_client = ClientBuilder::new(reqwest_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Self {
            base_url: base_url.to_string(),
            http_client: middleware_client,
        }
    }

    pub fn set_long_url<S: AsRef<str>>(&self, api: S, path: S, group: S, obj: Option<S>) -> String {
        let url: String;
        if let Some(ob_2) = obj {
            url = format!(
                "{}/{}/{}/{}/{}",
                self.base_url,
                api.as_ref(),
                urlencoding::encode(path.as_ref()),
                group.as_ref(),
                ob_2.as_ref()
            );
        } else {
            url = format!(
                "{}/{}/{}/{}",
                self.base_url,
                api.as_ref(),
                urlencoding::encode(path.as_ref()),
                group.as_ref()
            );
        }
        url
    }


    pub fn set_url<S: AsRef<str>>(&self, api: S, path: S) -> String {
        format!(
            "{}/{}/{}",
            self.base_url,
            api.as_ref(),
            urlencoding::encode(path.as_ref())
        )
    }


    pub fn set_short_url<S: AsRef<str>>(&self, api: S) -> String {
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
        server_port: 0,
        filesystem_root: fs_root.to_path_buf(),
        ..Default::default()
    }
}
