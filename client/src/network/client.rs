use std::ffi::OsStr;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use reqwest::{Client, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
// use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use tokio::sync::RwLock;
use tracing::{Level, instrument};
use urlencoding;

use super::APP_V1_BASE_URL;
use super::middleware::*;
use super::models::*;
use crate::error::FsModelError;
use crate::error::NetworkError;
use crate::fs_model::{FileAttr, Stats, attributes::SetAttr};

type Result<T> = std::result::Result<T, NetworkError>;
// type Result<T> = std::result::Result<T, FsModelError>;

#[derive(Debug, Clone)]
pub struct RemoteClient {
    base_url: String,
    http_client: ClientWithMiddleware,
    token_store: TokenStore,
}

impl RemoteClient {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<S: AsRef<str> + Debug>(base_url: S) -> Self {
        let token_store = Arc::new(RwLock::new(None));

        let reqwest_client = Client::new();
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

        let middleware_client = ClientBuilder::new(reqwest_client)
            // .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .with(AuthMiddleware::new(token_store.clone()))
            .build();

        Self {
            base_url: format!("{}{}", base_url.as_ref(), APP_V1_BASE_URL),
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
        return url;
    }

    fn set_short_url<S: AsRef<str>>(&self, api: S) -> String {
        let url = format!("{}/{}", self.base_url, api.as_ref());
        tracing::debug!("fetching {}", url);
        return url;
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_path<S: AsRef<str> + Debug>(
        &self,
        path: S,
    ) -> Result<Vec<SerializableFSItem>> {
        let url = self.set_url("list", path.as_ref());

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?; // propagate HTTP errors as errors

        match resp.status() {
            StatusCode::OK => {
                let body = resp
                    .json()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body);
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            StatusCode::BAD_REQUEST => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::InvalidInput(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow!(body)))
            }
        }

        // let body = resp.json().await?;
        // tracing::debug!("response: {:?}", body);

        // Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file<S: AsRef<str> + Debug>(
        &self,
        path: S,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let url = self.set_url("files", path.as_ref());

        let read_file = ReadFileRequest::new(offset, size);

        let resp = self
            .http_client
            .get(url)
            .json(&read_file)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?
            .error_for_status()?; // propagate HTTP errors as errors

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
        // .map_err(|_err| anyhow::anyhow!("Invalid base64 data"));

        Ok(bytes.to_vec())
    }

    #[instrument(skip(self, data), err(level = Level::ERROR))]
    pub async fn write_file<S: AsRef<str> + Debug>(
        &self,
        path: S,
        offset: usize,
        data: Vec<u8>,
    ) -> Result<FileAttr> {
        use reqwest::header::CONTENT_TYPE;

        let url = self.set_url("files", path.as_ref());

        let resp = self
            .http_client
            .put(url)
            .query(&[("offset", &offset.to_string())])
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(data)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?
            .error_for_status()?;

        let attr: FileAttr = resp.json().await?;
        Ok(attr)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir<S: AsRef<str> + Debug>(&self, path: S) -> Result<FileAttr> {
        let url = self.set_url("mkdir", path.as_ref());

        let resp = self
            .http_client
            .post(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);
        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename<S: AsRef<str> + Debug>(
        &self,
        old_path: S,
        new_path: S,
    ) -> anyhow::Result<()> {
        let url = self.set_short_url("rename");
        let rename_req = RenameRequest::new(
            String::from(old_path.as_ref()),
            String::from(new_path.as_ref()),
        );
        self.http_client
            .put(url)
            .json(&rename_req)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove<S: AsRef<str> + Debug>(&self, path: S) -> anyhow::Result<()> {
        let url = self.set_url("files", path.as_ref());
        self.http_client
            .delete(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child<S: AsRef<str> + Debug>(
        &self,
        uid: u32,
        gid: u32,
        path: S,
    ) -> anyhow::Result<FileAttr> {
        let url = self.set_url("attributes/directory", path.as_ref());

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes<S: AsRef<str> + Debug>(&self, path: S) -> Result<FileAttr> {
        let url = self.set_url("attributes", path.as_ref());

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => {
                let body = resp
                    .json()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body);
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::NotFound(body).to_string(),
                ))
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes<S: AsRef<str> + Debug>(
        &self,
        uid: u32,
        gid: u32,
        path: S,
        new_attributes: SetAttr,
    ) -> anyhow::Result<FileAttr> {
        let url = self.set_url("attributes", path.as_ref());

        let resp = self
            .http_client
            .put(url)
            .json(&SetAttrRequest::new(uid, gid, new_attributes))
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions<S: AsRef<str> + Debug>(&self, path: S) -> anyhow::Result<u32> {
        let url = self.set_url("permissions", path.as_ref());

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: u32 = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_stats<S: AsRef<str> + Debug>(&self, path: S) -> anyhow::Result<Stats> {
        let url = self.set_url("stats", path.as_ref());

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_symlink(&self, path: &str, target: &str) -> anyhow::Result<FileAttr> {
        let url = self.set_url("symlink", path);

        let req = WriteSymlink::new(target);

        let resp = self
            .http_client
            .post(url)
            .json(&req)
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_symlink(&self, path: &str) -> anyhow::Result<String> {
        let url = self.set_url("symlink", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let target: String = resp.json().await?;
        tracing::debug!("response: {:?}", target);
        Ok(target)
    }

    /* AUTHENTICATION MANAGEMENT */
    #[instrument(skip(self, password), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn login(&self, username: String, password: String) -> Result<String> {
        let url = self.set_short_url("auth/login");

        let resp = self
            .http_client
            .post(url)
            .json(&LoginRequest::new(username, password))
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => {
                let body: LoginResponse = resp
                    .json()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                tracing::debug!("response: {:?}", body);

                // Store the token
                let mut token_guard = self.token_store.write().await;
                *token_guard = Some(body.token.clone());
                drop(token_guard);

                tracing::info!("Login successful");
                return Ok(body.token);
            }
            StatusCode::UNAUTHORIZED => Err(NetworkError::ServerError(
                "Invalid credentials.".to_string(),
            )),
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn logout(&self) -> anyhow::Result<()> {
        let url = self.set_short_url("logout");

        let resp = self.http_client.post(url).send().await?;

        match resp.status() {
            StatusCode::OK => {
                // Clear the token
                let mut token_guard = self.token_store.write().await;
                *token_guard = None;
                drop(token_guard);

                Ok(())
            }
            _ => Err(anyhow!("Internal server error!")),
        }
    }

    /* XATTRIBUTES MANAGEMENT */
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(&self, path: &OsStr, name: &str) -> Result<Xattributes> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow!("Path is not valid UTF-8"))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = format!("{}/names/{}", url_1, name);

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => {
                let body: Xattributes = resp
                    .json()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                tracing::debug!("response: {:?}", body);
                Ok(body)
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::NotFound(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_x_attributes(
        &self,
        path: &OsStr,
        name: &str,
        xattributes: &[u8],
    ) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| NetworkError::ServerError(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = format!("{}/names/{}", url_1, name);

        let resp = self
            .http_client
            .put(url)
            .json(&Xattributes::new(xattributes))
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attributes(&self, path: &OsStr) -> Result<Vec<String>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| NetworkError::ServerError(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = format!("{}/names", url_1);

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => {
                let list_names: ListXattributes = resp
                    .json::<ListXattributes>()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                tracing::debug!("response: {:?}", list_names);
                Ok(list_names.names)
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::NotFound(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes(&self, path: &OsStr, name: &str) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| NetworkError::ServerError(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = format!("{}/names/{}", url_1, name);

        let resp = self
            .http_client
            .delete(url)
            .send()
            .await
            .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::ServerError(
                    FsModelError::PermissionDenied(body).to_string(),
                ))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| NetworkError::Other(anyhow::anyhow!(e)))?;
                Err(NetworkError::Other(anyhow::anyhow!(body)))
            }
        }
    }
}
