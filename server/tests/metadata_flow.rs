mod common;

#[tokio::test]
async fn test_set_and_get_xattr() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());

    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // mkdir
    let url = client.set_url("mkdir", "/dir");
    client.post(&url).send().await?.error_for_status()?;

    // set xattr
    let url = client.set_long_url("xattributes", "/dir", "names", Some("user.test"));
    let value_bytes = b"hello".to_vec();
    client.put(&url)
        .json(&serde_json::json!({
            "xattributes": value_bytes
        }))
        .send()
        .await?
        .error_for_status()?;

    // get xattr
    let url = client.set_long_url("xattributes", "/dir", "names", Some("user.test"));
    let res = client.get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let returned_bytes = res["xattributes"]
        .as_array()
        .expect("expected array")
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<u8>>();

    assert_eq!(returned_bytes, b"hello");

    Ok(())
}



#[tokio::test]
async fn test_set_and_delete_xattr() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // crea cartella
    let url = client.set_url("mkdir", "/dir_with_xattr");
    client.post(&url).send().await?.error_for_status()?;

    // set xattr
    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    let value_bytes = b"hello".to_vec();
    client.put(&url)
        .json(&serde_json::json!({"xattributes": value_bytes}))
        .send().await?.error_for_status()?;

    // verifica xattr
    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    let res = client.get(&url).send().await?.error_for_status()?
        .json::<serde_json::Value>().await?;

    let returned_bytes = res["xattributes"].as_array().unwrap()
        .iter().map(|v| v.as_u64().unwrap() as u8).collect::<Vec<u8>>();
    assert_eq!(returned_bytes, b"hello");

    // delete xattr
    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    client.delete(&url).send().await?.error_for_status()?;

    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    let res = client.get(&url).send().await?;
    assert_eq!(res.status(), reqwest::StatusCode::NO_CONTENT);

    Ok(())
}

