use async_trait;
use http::StatusCode;
use reqwest::{Client, Response, Result as ReqwestResult};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use std::fmt::Debug;
use tracing::{Level, instrument};
use urlencoding;

use super::APP_V1_BASE_URL;
use super::RemoteStorage;
use super::middleware::*;
use super::models::*;
use crate::error::{FuseError, NetworkError};
use crate::fs_model::{Attributes, Stats, attributes::SetAttr};

type Result<T> = std::result::Result<T, NetworkError>;

#[derive(Debug, Clone)]
pub struct RemoteClient {
    base_url: String,
    http_client: ClientWithMiddleware,
    token_store: TokenStore,
}

impl RemoteClient {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<S: AsRef<str> + Debug>(base_url: S) -> Self {
        let token_store = TokenStore::new();
        let reqwest_client = Client::new();
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

        let middleware_client = ClientBuilder::new(reqwest_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .with(AuthMiddleware::new(token_store.clone()))
            .build();

        Self {
            base_url: format!(
                "{}/{}",
                base_url.as_ref().trim_end_matches('/'),
                APP_V1_BASE_URL.trim_start_matches('/')
            ),
            http_client: middleware_client,
            token_store,
        }
    }

    fn set_url<S: AsRef<str>>(&self, api: S, path: S) -> String {
        let url = format!(
            "{}/{}/{}",
            self.base_url,
            api.as_ref(),
            urlencoding::encode(path.as_ref())
        );
        tracing::debug!("fetching {}", url);
        url
    }

    fn set_short_url<S: AsRef<str>>(&self, api: S) -> String {
        let url = format!("{}/{}", self.base_url, api.as_ref());
        tracing::debug!("fetching {}", url);
        url
    }

    fn set_long_url<S: AsRef<str>>(&self, api: S, path: S, group: S, obj: Option<S>) -> String {
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

        tracing::debug!("fetching {}", url);
        url
    }
}

#[async_trait::async_trait]
impl RemoteStorage for RemoteClient {
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn health_check(&self) -> Result<()> {
        let url = self.set_short_url("health");
        let resp = self.http_client.get(&url).send().await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    // AUTHENTICATION MANAGEMENT
    #[instrument(skip(self, password), err(level = Level::ERROR))]
    async fn login(&self, username: String, password: String) -> Result<String> {
        let url = self.set_short_url("auth/login");

        let resp = self
            .http_client
            .post(url)
            .json(&LoginRequest::new(username, password))
            .send()
            .await?;

        let body: LoginResponse = handle_response(resp, |r| r.json()).await?;

        // Store the token
        self.token_store.set_token(body.token.clone()).await;

        tracing::info!("Login successful");
        Ok(body.token)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn logout(&self) -> Result<()> {
        let url = self.set_short_url("auth/logout");

        let resp = self.http_client.post(url).send().await?;

        handle_response(resp, |_| async {
            // Clear the token
            self.token_store.clear_token().await;
            Ok(())
        })
        .await
    }

    // ATTRIBUTE
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn get_attributes(&self, path: &str) -> Result<Attributes> {
        let url = self.set_url("attributes", path);
        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn set_attributes(&self, path: &str, new_attributes: SetAttr) -> Result<Attributes> {
        let url = self.set_url("attributes", path);

        let resp = self
            .http_client
            .put(url)
            .json(&SetAttrRequest::new(new_attributes))
            .send()
            .await?;

        handle_response(resp, |r| r.json()).await
    }

    // XATTRIBUTES
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn get_x_attributes(&self, path: &str, name: &str) -> Result<Option<Xattributes>> {
        let url = self.set_long_url("xattributes", path, "names", Some(name));

        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| async {
            if r.status() == StatusCode::NO_CONTENT {
                Ok(None)
            } else {
                r.json().await.map(Some)
            }
        })
        .await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn set_x_attributes(&self, path: &str, name: &str, xattributes: &[u8]) -> Result<()> {
        let url = self.set_long_url("xattributes", path, "names", Some(name));

        let resp = self
            .http_client
            .put(url)
            .json(&Xattributes::new(xattributes))
            .send()
            .await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn list_x_attributes(&self, path: &str) -> Result<Vec<String>> {
        let url = self.set_long_url("xattributes", path, "names", None);

        let resp = self.http_client.get(url).send().await?;

        let list_names: ListXattributes = handle_response(resp, |r| r.json()).await?;
        Ok(list_names.names)
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn remove_x_attributes(&self, path: &str, name: &str) -> Result<()> {
        let url = self.set_long_url("xattributes", path, "names", Some(name));

        let resp = self.http_client.delete(url).send().await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    // PERMISSIONS AND STATS
    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn get_permissions(&self, path: &str, mask: u32) -> Result<()> {
        let url = self.set_url("permissions", path);
        let resp = self
            .http_client
            .get(url)
            .query(&[("mask", &mask.to_string())])
            .send()
            .await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn get_stats(&self, path: &str) -> Result<Stats> {
        let url = self.set_url("stats", path);
        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    // FILESYSTEM OPERATIONS
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn list_path(&self, path: &str) -> Result<Vec<SerializableFSItem>> {
        let url = self.set_url("list", path);
        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn read_file(&self, path: &str, offset: usize, size: usize) -> Result<Vec<u8>> {
        let url = self.set_url("files", path);
        let read_file = ReadFileRequest::new(offset, size);

        let resp = self.http_client.get(url).json(&read_file).send().await?;

        let bytes = handle_response(resp, |r| r.bytes()).await?;
        Ok(bytes.to_vec())
    }

    #[instrument(skip(self, data), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn write_file(&self, path: &str, offset: usize, data: &[u8]) -> Result<Attributes> {
        use reqwest::header::CONTENT_TYPE;

        let url = self.set_url("files", path);

        let resp = self
            .http_client
            .put(url)
            .query(&[("offset", &offset.to_string())])
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data.to_vec())
            .send()
            .await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn mkdir(&self, path: &str) -> Result<Attributes> {
        let url = self.set_url("mkdir", path);
        let resp = self.http_client.post(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn rename(&self, old_path: &str, new_path: &str) -> Result<()> {
        let url = self.set_short_url("rename");
        let rename_req = RenameRequest::new(String::from(old_path), String::from(new_path));

        let resp = self.http_client.put(url).json(&rename_req).send().await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn remove(&self, path: &str) -> Result<()> {
        let url = self.set_url("files", path);
        let resp = self.http_client.delete(url).send().await?;

        handle_response(resp, |_| async { Ok(()) }).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn resolve_child(&self, path: &str) -> Result<Attributes> {
        let url = self.set_url("attributes/directory", path);
        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn create_symlink(&self, path: &str, target: &str) -> Result<Attributes> {
        let url = self.set_url("symlink", path);
        let req = WriteSymlink::new(target);

        let resp = self.http_client.post(url).json(&req).send().await?;

        handle_response(resp, |r| r.json()).await
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    async fn read_symlink(&self, path: &str) -> Result<String> {
        let url = self.set_url("symlink", path);
        let resp = self.http_client.get(url).send().await?;

        handle_response(resp, |r| r.json()).await
    }
}

// Generic helper to handle the response
async fn handle_response<F, T, Fut>(resp: Response, extractor: F) -> Result<T>
where
    F: FnOnce(Response) -> Fut,
    Fut: Future<Output = ReqwestResult<T>>,
{
    if !resp.status().is_success() {
        // deserialize FuseError from the response body
        let api_error: FuseError = resp.json().await.map_err(|e| {
            tracing::error!("Failed to parse error JSON: {}", e);
            NetworkError::UnexpectedResponse("Failed to parse remote error".to_string())
        })?;

        return Err(NetworkError::ServerError(api_error));
    }

    let data = extractor(resp)
        .await
        .map_err(|e| NetworkError::UnexpectedResponse(e.to_string()))?;

    Ok(data)
}
