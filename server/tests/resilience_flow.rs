mod common;
use bytes::Bytes;
use crate::common::{SetAttr, SetAttrRequest};

#[tokio::test]
async fn test_invalid_url() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;


    // URL inesistente
    let resp = client.get(&client.set_short_url("nonexistent")).send().await?;
    assert_eq!(resp.status(), 400);

    Ok(())
}


#[tokio::test]
async fn test_invalid_path_list() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;


    // path non valido per list
    let url = client.set_url("list", "/path/nonexistent");
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), 404); // Not Found

    Ok(())
}


#[tokio::test]
async fn write_on_nonexistent_path_returns_not_found() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = format!(
        "{}?offset=0",
        client.set_url("files", "/no/such/dir/file.txt")
    );

    let resp = client.put(&url)
        .body(Bytes::from_static(b"data"))
        .send().await?;

    assert_eq!(resp.status(), 404);
    Ok(())
}

#[tokio::test]
async fn write_file_without_permissions_fails() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // create file
    let url = format!("{}?offset=0", client.set_url("files", "/ro.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"data"))
        .send()
        .await?
        .error_for_status()?;

    // chmod 400
    let attr_url = client.set_url("attributes", "/ro.txt");
    let req = SetAttrRequest::new(SetAttr {
        mode: Some(0o400),
        ..Default::default()
    });
    client.put(&attr_url)
        .json(&req)
        .send()
        .await?
        .error_for_status()?;

    // try to write again
    let url = format!("{}?offset=0", client.set_url("files", "/ro.txt"));
    let resp = client.put(&url)
        .body(Bytes::from_static(b"x"))
        .send()
        .await?;

    assert_eq!(resp.status(), 403);
    Ok(())
}


