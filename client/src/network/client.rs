use std::ffi::OsStr;

use anyhow;
use reqwest::Client;
use tracing::{Level, instrument};
use urlencoding;

use super::models::*;

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
    pub async fn list_path(&self, path: &OsStr) -> anyhow::Result<Vec<SerializableFSItem>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("list", path_str);

        let resp = self.http_client.get(url).send().await?.error_for_status()?; // propagate HTTP errors as errors
        let body = resp.json().await?;

        tracing::debug!("response: {:?}", body);

        Ok(body)
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
    pub async fn mkdir(&self, path: &OsStr) -> anyhow::Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;

        let url = self.set_url("mkdir", path_str);

        self.http_client.post(url).send().await?.error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &OsStr, new_path: &OsStr) -> anyhow::Result<()> {
        let old_path_str = old_path.to_str().ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let new_path_str = new_path.to_str().ok_or_else(|| anyhow::anyhow!("Path is not valid UTF-8"))?;
        let url = self.set_short_url("rename");
        let rename_req = RenameRequest::new(String::from(old_path_str), String::from(new_path_str));
        self.http_client.post(url).json(&rename_req).send().await?.error_for_status()?;
        Ok(())
    }
}
