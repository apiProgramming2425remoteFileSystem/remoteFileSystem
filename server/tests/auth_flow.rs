mod common;

use crate::common::{HttpClient, TEST_PASSWORD, TEST_USER};
use serde::{Serialize, Deserialize};

#[tokio::test]
async fn login_with_valid_credentials_succeeds() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = client.set_short_url("auth/login");
    let resp = client.post(&url)
        .json(&LoginRequest::new(TEST_USER.into(), TEST_PASSWORD.into()))
        .send()
        .await?;

    assert_eq!(resp.status(), 200);
    let token = resp.json::<Token>().await?;
    assert!(!token.token.is_empty());

    Ok(())
}


#[tokio::test]
async fn login_with_wrong_password_fails() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = client.set_short_url("auth/login");
    let resp = client.post(&url)
        .json(
            &LoginRequest::new(TEST_USER.into(), "wrong_password".into())
        )
        .send()
        .await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}

#[tokio::test]
async fn access_without_token_is_unauthorized() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());

    let (_client_with_token, _handle, _tmpdir) =
        common::start_server_app(config).await?;

    // nuovo client SENZA token, ma stessa base_url
    let client = HttpClient::new_with_token(
        &_client_with_token.base_url,
        None,
    );

    let url = client.set_url("list", "/");
    let resp = client.get(&url).send().await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}


#[tokio::test]
async fn access_with_fake_token_is_unauthorized() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());

    let (_client_with_token, _handle, _tmpdir) =
        common::start_server_app(config).await?;

    let client = HttpClient::new_with_token(
        &_client_with_token.base_url,
        Some("this.is.a.fake.token"),
    );

    let url = client.set_url("list", "/");
    let resp = client.get(&url).send().await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}

#[tokio::test]
async fn logout_revokes_token() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // logout
    let url = client.set_short_url("auth/logout");
    let resp = client.post(&url).send().await?;
    assert_eq!(resp.status(), 200);

    // try using same token again
    let url = client.set_url("list", "/");
    let resp = client.get(&url).send().await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}


#[derive(Debug, Serialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

impl LoginRequest {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

#[derive(Debug, Deserialize)]
pub struct Token {
    token: String,
}
