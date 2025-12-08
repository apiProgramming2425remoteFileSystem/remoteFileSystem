use std::ffi::OsStr;
use std::fmt::Debug;

use anyhow;
use reqwest::Client;
use tracing::{Level, instrument};
use urlencoding;

use super::APP_V1_BASE_URL;
use super::models::*;
use crate::error::NetworkError;
use crate::fs_model::{FileAttr, Stats, attributes::SetAttr};

type Result<T> = std::result::Result<T, NetworkError>;

#[derive(Debug)]
pub struct RemoteClient {
    base_url: String,
    http_client: Client,
}

fn path_to_str<S: AsRef<OsStr>>(path: S) -> Result<String> {
    path.as_ref()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| NetworkError::Other(anyhow::format_err!("Path is not valid UTF-8")))
}

impl RemoteClient {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new<S: AsRef<str> + Debug>(base_url: S) -> Self {
        Self {
            base_url: format!("{}{}", base_url.as_ref(), APP_V1_BASE_URL),
            http_client: Client::new(),
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
    pub async fn list_path<S: AsRef<OsStr> + Debug>(
        &self,
        path: S,
    ) -> Result<Vec<SerializableFSItem>> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("list", &path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?; // propagate HTTP errors as errors
        let body = resp.json().await?;

        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn read_file<S: AsRef<str> + Debug>(
        &self,
        path: S,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let url = self.set_url("files", path.as_ref());

        let resp = self.http_client.get(url).send().await?.error_for_status()?; // propagate HTTP errors as errors

        let body: ReadFile = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        return body
            .data()
            .map_err(|_err| NetworkError::Other(anyhow::format_err!("Invalid base64 data")));
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn write_file<S: AsRef<str> + Debug>(
        &self,
        path: S,
        offset: usize,
        data: &[u8],
    ) -> Result<()> {
        let url = self.set_url("files", path.as_ref());

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
    pub async fn mkdir<S: AsRef<OsStr> + Debug>(&self, path: S) -> Result<FileAttr> {
        let path_str = path_to_str(path)?;
        let url = self.set_url("mkdir", &path_str);

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
    pub async fn rename<S: AsRef<OsStr> + Debug>(&self, old_path: S, new_path: S) -> Result<()> {
        let old_path_str = path_to_str(old_path)?;
        let new_path_str = path_to_str(new_path)?;
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
    pub async fn remove<S: AsRef<OsStr> + Debug>(&self, path: S) -> Result<()> {
        let path_str = path_to_str(path)?;
        let url = self.set_url("files", &path_str);
        self.http_client
            .delete(url)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn resolve_child<S: AsRef<OsStr> + Debug>(
        &self,
        uid: u32,
        gid: u32,
        path: S,
    ) -> Result<FileAttr> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("attributes/directory", &path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes<S: AsRef<OsStr> + Debug>(&self, path: S) -> Result<FileAttr> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("attributes", &path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes<S: AsRef<OsStr> + Debug>(
        &self,
        uid: u32,
        gid: u32,
        path: S,
        new_attributes: SetAttr,
    ) -> Result<FileAttr> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("attributes", &path_str);

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
    pub async fn get_permissions<S: AsRef<OsStr> + Debug>(&self, path: S) -> Result<u32> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("permissions", &path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: u32 = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_stats<S: AsRef<OsStr> + Debug>(&self, path: S) -> Result<Stats> {
        let path_str = path_to_str(path)?;

        let url = self.set_url("stats", &path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }
}
