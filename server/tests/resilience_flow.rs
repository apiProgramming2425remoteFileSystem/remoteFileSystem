mod common;

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