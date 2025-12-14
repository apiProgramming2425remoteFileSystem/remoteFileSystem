use std::ffi::OsStr;
use std::sync::Arc;

use anyhow::anyhow;
use reqwest::{Client, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
// use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use tokio::sync::RwLock;
use tracing::{Level, instrument};
use urlencoding;

use crate::error::FsModelError;
use crate::fs_model::{FileAttr, Stats, attributes::SetAttr};

use super::middleware::*;
use super::models::*;

type Result<T> = std::result::Result<T, FsModelError>;

#[derive(Debug)]
pub struct RemoteClient {
    base_url: String,
    http_client: ClientWithMiddleware,
    token_store: TokenStore,
}

impl RemoteClient {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(base_url: &str) -> Self {
        let token_store = Arc::new(RwLock::new(None));

        let reqwest_client = Client::new();
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

        let middleware_client = ClientBuilder::new(reqwest_client)
            // .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .with(AuthMiddleware::new(token_store.clone()))
            .build();

        Self {
            base_url: base_url.to_string(),
            http_client: middleware_client,
            token_store,
        }
    }

    fn set_url(&self, api: &str, path: &str) -> String {
        let url = format!("{}/{}/{}", self.base_url, api, urlencoding::encode(path));
        tracing::debug!("fetching {}", url);
        return url;
    }

    fn set_short_url(&self, api: &str) -> String {
        let url = format!("{}/{}", self.base_url, api);
        tracing::debug!("fetching {}", url);
        return url;
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_path(&self, path: &OsStr) -> Result<Vec<SerializableFSItem>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("list", path_str);

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?; // propagate HTTP errors as errors

        match resp.status() {
            StatusCode::OK => {
                let body = resp
                    .json()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body);
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            StatusCode::BAD_REQUEST => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::InvalidInput(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }

        // let body = resp.json().await?;
        // tracing::debug!("response: {:?}", body);

        // Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file(
        &self,
        path: &str,
        offset: usize,
        size: usize,
    ) -> anyhow::Result<Vec<u8>> {
        let url = self.set_url("files", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?; // propagate HTTP errors as errors

        let body: ReadFile = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        return body
            .data()
            .map_err(|_err| anyhow::anyhow!("Invalid base64 data"));
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn write_file(&self, path: &str, offset: usize, data: &[u8]) -> anyhow::Result<()> {
        let url = self.set_url("files", path);

        let write_file = WriteFile::new(offset, data);

        let resp = self
            .http_client
            .put(url)
            .json(&write_file)
            .send()
            .await?
            .error_for_status()?; // propagate HTTP errors as errors

        let body = resp.text().await?;
        tracing::debug!("response: {:?}", body);

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &OsStr) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("mkdir", path_str);

        let resp = self
            .http_client
            .post(url)
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);
        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr) -> anyhow::Result<()> {
        let old_path_str = old_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let new_path_str = new_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let url = self.set_short_url("rename");
        let rename_req = RenameRequest::new(String::from(old_path_str), String::from(new_path_str));
        self.http_client
            .put(url)
            .json(&rename_req)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &OsStr) -> anyhow::Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let url = self.set_url("files", path_str);
        self.http_client
            .delete(url)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
    ) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes/directory", path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &OsStr) -> Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes", path_str);

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let body = resp
                    .json()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body);
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes(
        &self,
        uid: u32,
        gid: u32,
        path: &OsStr,
        new_attributes: SetAttr,
    ) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes", path_str);

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
    pub async fn get_permissions(&self, path: &OsStr) -> anyhow::Result<u32> {
        let path_str: &str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("permissions", path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: u32 = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_stats(&self, path: &OsStr) -> anyhow::Result<Stats> {
        let path_str: &str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("stats", path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    /* AUTHENTICATION MANAGEMENT */
    #[instrument(skip(self, password), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn login(&self, username: String, password: String) -> Result<String> {
        let url = self.set_short_url("login");

        let resp = self
            .http_client
            .post(url)
            .json(&LoginRequest::new(username, password))
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let body: LoginResponse = resp
                    .json()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);

                // Store the token
                let mut token_guard = self.token_store.write().await;
                *token_guard = Some(body.token.clone());
                drop(token_guard);

                tracing::info!("Login successful");
                return Ok(body.token);
            }
            StatusCode::UNAUTHORIZED => Err(FsModelError::PermissionDenied(String::from(
                "Invalid credentials.",
            ))),
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
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
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let body: Xattributes = resp
                    .json()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                Ok(body)
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
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
        let path_str = path.to_str().ok_or_else(|| {
            FsModelError::ConversionFailed(String::from("Path is not valid UTF-8"))
        })?;

        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self
            .http_client
            .put(url)
            .json(&Xattributes::new(xattributes))
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attributes(&self, path: &OsStr) -> Result<Vec<String>> {
        let path_str = path.to_str().ok_or_else(|| {
            FsModelError::ConversionFailed(String::from("Path is not valid UTF-8"))
        })?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = self.set_url(&url_1, "names");

        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let list_names: ListXattributes = resp
                    .json::<ListXattributes>()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", list_names);
                Ok(list_names.names)
            }
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            StatusCode::NOT_FOUND => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes(&self, path: &OsStr, name: &str) -> Result<()> {
        let path_str = path.to_str().ok_or_else(|| {
            FsModelError::ConversionFailed(String::from("Path is not valid UTF-8"))
        })?;

        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self
            .http_client
            .delete(url)
            .send()
            .await
            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            _ => {
                let body: String = resp
                    .text()
                    .await
                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }
    }
}
