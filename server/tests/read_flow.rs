mod common;
use bytes::Bytes;

#[tokio::test]
async fn test_list_directories() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // crea cartelle
    let url = client.set_url("mkdir", "/folderA");
    client.post(&url).send().await?.error_for_status()?;

    let url = client.set_url("mkdir", "/folderB");
    client.post(&url).send().await?.error_for_status()?;

    // lista root
    let url = client.set_url("list", "/");
    let res = client.get(&url)
        .send().await?.error_for_status()?
        .json::<Vec<serde_json::Value>>().await?;

    let names: Vec<_> = res.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"folderA"));
    assert!(names.contains(&"folderB"));

    Ok(())
}



#[tokio::test]
async fn test_list_files() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // Crea directory
    let url = client.set_url("mkdir", "/mydir");
    client.post(&url).send().await?.error_for_status()?;

    // Scrivi un file
    let url_write = client.set_url("files", "/mydir/file1.txt");
    let url_with_query = format!("{}?offset=0", url_write);
    client.put(&url_with_query)
        .body(Bytes::from("Hello, world!"))
        .send()
        .await?
        .error_for_status()?;

    // Lista directory
    let url = client.set_url("list", "/mydir");
    let res = client.get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<serde_json::Value>>()
        .await?;

    assert_eq!(res.len(), 1);
    assert_eq!(res[0]["name"], "file1.txt");

    Ok(())
}
