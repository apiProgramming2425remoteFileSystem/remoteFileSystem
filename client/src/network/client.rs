/*use anyhow;
use reqwest::Client;
use urlencoding;

use super::models::*;

#[derive(Debug)]
pub struct RemoteClient {
    base_url: String,
    http_client: Client,
}

impl RemoteClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http_client: Client::new(),
        }
    }

    pub async fn list_path(&self, path: &str) -> anyhow::Result<Vec<SerializableFSItem>> {
        let url = format!("{}/list/{}", self.base_url, urlencoding::encode(path));

        tracing::debug!("fetching {}", url);

        let resp = self.http_client.get(url).send().await?.error_for_status()?; // propagate HTTP errors as errors
        let body = resp.json().await?;

        tracing::debug!("response: {:?}", body);

        Ok(body)
    }
}
*/