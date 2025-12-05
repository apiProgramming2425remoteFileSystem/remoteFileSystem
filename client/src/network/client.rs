use std::ffi::OsStr;
use std::path::Path;
use anyhow;
use reqwest::Client;
use tracing::{Level, instrument};
use urlencoding;

use crate::fs_model::{FileAttr, Stats, attributes::SetAttr};

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
    pub async fn list_path(&self, path: &str) -> anyhow::Result<Vec<SerializableFSItem>> {
        let url = self.set_url("list", path);

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
    pub async fn write_file(&self, path: &str, offset: usize, data: &[u8]) -> anyhow::Result<FileAttr> {
        let url = self.set_url("files", path);

        let write_file = WriteFile::new(offset, data);

        let resp = self
            .http_client
            .put(url)
            .json(&write_file)
            .send()
            .await?
            .error_for_status()?; // propagate HTTP errors as errors

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn mkdir(&self, path: &str) -> anyhow::Result<FileAttr> {
        let url = self.set_url("mkdir", path);

        let resp = self.http_client
            .post(url)
            .send()
            .await?
            .error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);
        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn rename(&self, old_path: &str, new_path: &str) -> anyhow::Result<()> {
        let url = self.set_short_url("rename");
        let rename_req = RenameRequest::new(String::from(old_path), String::from(new_path));
        self.http_client
            .put(url)
            .json(&rename_req)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn remove(&self, path: &str) -> anyhow::Result<()> {
        let url = self.set_url("files", path);
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
        path: &str,
    ) -> anyhow::Result<FileAttr> {
        let url = self.set_url("attributes/directory", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_attributes(&self, path: &str) -> anyhow::Result<FileAttr> {
        let url = self.set_url("attributes", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: FileAttr = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn set_attributes(
        &self,
        uid: u32,
        gid: u32,
        path: &str,
        new_attributes: SetAttr,
    ) -> anyhow::Result<FileAttr> {
        let url = self.set_url("attributes", path);

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
    pub async fn get_permissions(&self, path: &str) -> anyhow::Result<u32> {
        let url = self.set_url("permissions", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body: u32 = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_stats(&self, path: &str) -> anyhow::Result<Stats> {
        let url = self.set_url("stats", path);

        let resp = self.http_client.get(url).send().await?.error_for_status()?;

        let body = resp.json().await?;
        tracing::debug!("response: {:?}", body);

        Ok(body)
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn create_symlink(&self, path: &str, target: &str) -> anyhow::Result<FileAttr> {
        let url = self.set_url("symlink", path);

        let req = WriteSymlink::new(target);

        let resp = self.http_client
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

        let resp = self.http_client
            .get(url)
            .send()
            .await?
            .error_for_status()?;

        let target: String = resp.json().await?;
        tracing::debug!("response: {:?}", target);
        Ok(target)
    }

}
