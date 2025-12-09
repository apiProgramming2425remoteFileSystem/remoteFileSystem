use std::ffi::OsStr;

use anyhow::anyhow;
use reqwest::{Client, StatusCode};
use tracing::{Level, instrument};
use urlencoding;

use crate::error::FsModelError;
use crate::fs_model::{FileAttr, Stats, attributes::SetAttr};

use super::models::*;

type Result<T> = std::result::Result<T, FsModelError>;

#[derive(Debug)]
pub struct RemoteClient {
    base_url: String,
    http_client: Client,
}

impl RemoteClient {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            http_client: Client::new(),
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
    pub async fn list_path(&self, path: &OsStr, token: &str) -> Result<Vec<SerializableFSItem>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("list", path_str);

        let resp = self.http_client.get(url)
                                .header("Authorization", format!("Bearer {}", token))
                                .send()
                                .await
                                .map_err(|e| FsModelError::ClientError(e.to_string()))?; // propagate HTTP errors as errors

        match resp.status() {
            StatusCode::OK => {
                let body = resp.json().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body)
            },
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            },
            StatusCode::BAD_REQUEST =>{
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::InvalidInput(body))
            },
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
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
        token: &str
    ) -> anyhow::Result<Vec<u8>> {
        let url = self.set_url("files", path);

        let resp = self.http_client.get(url).header("Authorization", format!("Bearer {}", token)).send().await?.error_for_status()?; // propagate HTTP errors as errors

        let body: ReadFile = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        return body
            .data()
            .map_err(|_err| anyhow::anyhow!("Invalid base64 data"));
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn write_file(&self, path: &str, offset: usize, data: &[u8], token: &str) -> anyhow::Result<()> {
        let url = self.set_url("files", path);

        let write_file = WriteFile::new(offset, data);

        let resp = self
            .http_client
            .put(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&write_file)
            .send()
            .await?
            .error_for_status()?; // propagate HTTP errors as errors

        let body = resp.text().await?;
        tracing::debug!("response: {:?}", body);

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &OsStr, token: &str) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("mkdir", path_str);

        let resp = self.http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);
        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr, token: &str) -> anyhow::Result<()> {
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
            .header("Authorization", format!("Bearer {}", token))
            .json(&rename_req)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &OsStr, token: &str) -> anyhow::Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let url = self.set_url("files", path_str);
        self.http_client
            .delete(url)
            .header("Authorization", format!("Bearer {}", token))
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
        token: &str
    ) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes/directory", path_str);

        let resp = self.http_client.get(url).header("Authorization", format!("Bearer {}", token)).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &OsStr, token: &str) -> Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes", path_str);

        let resp = self.http_client
                            .get(url)
                            .header("Authorization", format!("Bearer {}", token))
                            .send()
                            .await
                            .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let body = resp.json().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body)
            },
            StatusCode::NOT_FOUND => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            },
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            }
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
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
        token: &str
    ) -> anyhow::Result<FileAttr> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("attributes", path_str);

        let resp = self
            .http_client
            .put(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&SetAttrRequest::new(uid, gid, new_attributes))
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_permissions(&self, path: &OsStr, token: &str) -> anyhow::Result<u32> {
        let path_str: &str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("permissions", path_str);

        let resp = self.http_client.get(url).header("Authorization", format!("Bearer {}", token)).send().await?.error_for_status()?;

        let body: u32 = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_stats(&self, path: &OsStr, token: &str) -> anyhow::Result<Stats> {
        let path_str: &str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("stats", path_str);

        let resp = self.http_client
                            .get(url)
                            .header("Authorization", format!("Bearer {}", token))
                            .send()
                            .await?
                            .error_for_status()?;

        let body = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    /* AUTHENTICATION MANAGEMENT */
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn login(&self, username: String, password: String) -> Result<String>{
        let url = self.set_short_url("login");

        let resp = self.http_client.post(url)
                                    .json(&LoginRequest::new(username, password))
                                    .send()
                                    .await
                                    .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => {
                let body: LoginResponse = resp.json().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                return Ok(body.token)
            }
            StatusCode::UNAUTHORIZED => {
                Err(FsModelError::PermissionDenied(String::from("Invalid credentials.")))
            }
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            }
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn logout(&self, token: &str) -> anyhow::Result<()>{
        let url = self.set_short_url("logout");

        let resp = self.http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        match resp.status(){
            StatusCode::OK => Ok(()),
            _ => Err(anyhow!("Internal server error!")),
        }
    }

    /* XATTRIBUTES MANAGEMENT */
    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(&self, path: &OsStr, name: &str, token: &str) -> Result<Xattributes>{
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;


        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self.http_client
                                .get(url)
                                .header("Authorization", format!("Bearer {}", token))
                                .send()
                                .await
                                .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status(){
            StatusCode::OK => {
                let body: Xattributes = resp.json().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", body);
                Ok(body)
            },
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            },
            StatusCode::NOT_FOUND => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            },
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body))
            )},
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_x_attributes(&self, path: &OsStr, name: &str, xattributes: &[u8], token: &str) -> Result<()>{
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::ConversionFailed(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self.http_client
                                .put(url)
                                .header("Authorization", format!("Bearer {}", token))
                                .json(&Xattributes::new(xattributes))
                                .send()
                                .await
                                .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            },
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            },
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attributes(&self, path: &OsStr, token: &str) -> Result<Vec<String>>{
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::ConversionFailed(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url = self.set_url(&url_1, "names");

        let resp = self.http_client
                                .get(url)
                                .header("Authorization", format!("Bearer {}", token))
                                .send()
                                .await
                                .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK =>  {
                let list_names: ListXattributes = resp.json::<ListXattributes>().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                tracing::debug!("response: {:?}", list_names);
                Ok(list_names.names)
            },
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            },
            StatusCode::NOT_FOUND => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::NotFound(body))
            },
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            },
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes(&self, path: &OsStr, name: &str, token: &str) -> Result<()>{
        let path_str = path
            .to_str()
            .ok_or_else(|| FsModelError::ConversionFailed(String::from("Path is not valid UTF-8")))?;

        let url_1 = self.set_url("xattributes", path_str);
        let url_2 = self.set_url(&url_1, "names");
        let url = self.set_url(&url_2, name);

        let resp = self.http_client
                                .delete(url)
                                .header("Authorization", format!("Bearer {}", token))
                                .send()
                                .await
                                .map_err(|e| FsModelError::ClientError(e.to_string()))?;

        match resp.status() {
            StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::PermissionDenied(body))
            },
            _ => {
                let body: String = resp.text().await.map_err(|e| FsModelError::ClientError(e.to_string()))?;
                Err(FsModelError::Backend(anyhow!(body)))
            },
        }
    }
}
