mod common;
use bytes::Bytes;
use serde::Serialize;
use crate::common::ReadFileRequest;

#[tokio::test]
async fn test_make_directory() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());

    // Avvia server e ottieni client già con token
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // POST mkdir
    let url = client.set_url("mkdir", "/folder1");
    let resp = client.post(&url).send().await?;
    assert!(resp.status().is_success());

    // GET list
    let url = client.set_url("list", "/folder1");
    let res = client.get(&url).send().await?;
    assert!(res.status().is_success());

    Ok(())
}


#[tokio::test]
async fn test_delete_directory() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // crea cartella
    let url = client.set_url("mkdir", "/to_delete");
    client.post(&url).send().await?.error_for_status()?;

    // delete
    let url = client.set_url("files", "/to_delete");
    client.delete(&url).send().await?.error_for_status()?;

    // lista root, non deve esserci
    let url = client.set_url("list", "/");
    let res = client.get(&url).send().await?.error_for_status()?
        .json::<Vec<serde_json::Value>>().await?;

    let names: Vec<_> = res.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(!names.contains(&"to_delete"));

    Ok(())
}



#[tokio::test]
async fn test_rename_directory() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    let url = client.set_url("mkdir", "/oldname");
    client.post(&url).send().await?.error_for_status()?;

    let url = client.set_short_url("rename");
    client.put(&url)
        .json(&serde_json::json!({
            "old_path": "/oldname",
            "new_path": "/newname",
            "flags": 0
        }))
        .send().await?.error_for_status()?;

    let url = client.set_url("list", "/");
    let res = client.get(&url).send().await?.error_for_status()?
        .json::<Vec<serde_json::Value>>().await?;

    let names: Vec<_> = res.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"newname"));
    assert!(!names.contains(&"oldname"));

    Ok(())
}


#[tokio::test]
async fn test_symlink_creation_and_read() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // crea target
    let url = client.set_url("mkdir", "/target_dir");
    client.post(&url).send().await?.error_for_status()?;

    // crea symlink
    let url = client.set_url("symlink", "/link_to_target");
    client.post(&url)
        .json(&serde_json::json!({"target": "/target_dir"}))
        .send().await?.error_for_status()?;

    // leggi symlink
    let url = client.set_url("symlink", "/link_to_target");
    let res = client.get(&url).send().await?.error_for_status()?
        .json::<serde_json::Value>().await?;

    assert_eq!(res.as_str().unwrap(), "/target_dir");

    Ok(())
}


#[tokio::test]
async fn test_delete_file() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // Crea directory e file
    client.post(&client.set_url("mkdir", "/d")).send().await?.error_for_status()?;

    let url_write = client.set_url("files", "/d/f.txt");
    let url_with_query = format!("{}?offset=0", url_write);
    client.put(&url_with_query)
        .body(Bytes::from("test"))
        .send()
        .await?
        .error_for_status()?;

    // Cancella file
    client.delete(&client.set_url("files", "/d/f.txt"))
        .send()
        .await?
        .error_for_status()?;

    // Lista directory -> non deve più esserci file
    let url = client.set_url("list", "/d");
    let res = client.get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<serde_json::Value>>()
        .await?;

    assert!(res.is_empty());

    Ok(())
}



#[tokio::test]
async fn test_move_file_between_dirs() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;

    // Crea due directory
    client.post(&client.set_url("mkdir", "/src")).send().await?.error_for_status()?;
    client.post(&client.set_url("mkdir", "/dst")).send().await?.error_for_status()?;

    // Scrivi file in src
    let url_write = client.set_url("files", "/src/f.txt");
    let url_with_query = format!("{}?offset=0", url_write);
    client.put(&url_with_query)
        .body(Bytes::from("move me"))
        .send()
        .await?
        .error_for_status()?;

    // Sposta file da src -> dst (usa endpoint rename)
    let url = client.set_short_url("rename");
    client.put(&url)
        .json(&serde_json::json!({
            "old_path": "/src/f.txt",
            "new_path": "/dst/f.txt",
            "flags": 0
        }))
        .send()
        .await?
        .error_for_status()?;

    // Controlla src -> deve essere vuota
    let res = client.get(&client.set_url("list", "/src"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<serde_json::Value>>()
        .await?;
    assert!(res.is_empty());

    // Controlla dst -> deve esserci file
    let res = client.get(&client.set_url("list", "/dst"))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<serde_json::Value>>()
        .await?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0]["name"], "f.txt");

    Ok(())
}

#[tokio::test]
async fn test_delete_file_and_directory() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let config = common::get_config(tmp_dir.path());
    let (client, _handle, _tmpdir)= common::start_server_app(config).await?;


    // crea cartella + file
    let _ = client.post(&client.set_url("mkdir", "/tmpDir")).send().await?;
    let url = format!("{}?offset=0", client.set_url("files", "/tmpDir/file.txt"));
    let _ = client.put(&url).body(Bytes::from("data")).send().await?.error_for_status()?;

    // cancella file
    let resp = client.delete(&client.set_url("files", "/tmpDir/file.txt")).send().await?;
    assert!(resp.status().is_success());

    // cancella cartella
    let resp = client.delete(&client.set_url("files", "/tmpDir")).send().await?;
    assert!(resp.status().is_success());

    Ok(())
}

#[tokio::test]
async fn write_with_offset_overwrites_only_part() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // prima scrittura
    let url = format!("{}?offset=0", client.set_url("files", "/b.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"abcdef"))
        .send().await?
        .error_for_status()?;

    // sovrascrivo da offset 2
    let url = format!("{}?offset=2", client.set_url("files", "/b.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"ZZ"))
        .send().await?
        .error_for_status()?;

    // leggo tutto
    let read_url = client.set_url("files", "/b.txt");
    let data = client.get(&read_url)
        .json(&ReadFileRequest::new(0, 20))
        .send().await?
        .error_for_status()?
        .bytes()
        .await?;

    assert_eq!(&data[..], b"abZZef");
    Ok(())
}

#[tokio::test]
async fn write_with_offset_beyond_eof_pads_with_zeros() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // write at offset 10
    let write_url = format!(
        "{}?offset=10",
        client.set_url("files", "/c.txt")
    );

    client.put(&write_url)
        .body(Bytes::from_static(b"x"))
        .send()
        .await?
        .error_for_status()?; // IMPORTANT: must succeed

    // read whole file
    let read_url = client.set_url("files", "/c.txt");
    let data = client.get(&read_url)
        .json(&ReadFileRequest::new(0, 20))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    // file must be 11 bytes long
    assert_eq!(data.len(), 11);

    // first 10 bytes must be zero
    assert!(data[..10].iter().all(|b| *b == 0));

    // last byte must be 'x'
    assert_eq!(data[10], b'x');

    Ok(())
}


#[tokio::test]
async fn rename_exchange_swaps_source_and_target() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    client.post(&client.set_url("mkdir", "/A")).send().await?.error_for_status()?;
    client.post(&client.set_url("mkdir", "/B")).send().await?.error_for_status()?;

    // metti un file in A
    let url_write = format!("{}?offset=0", client.set_url("files", "/A/file.txt"));
    client.put(&url_write).body(Bytes::from("x")).send().await?.error_for_status()?;

    // exchange
    let url = client.set_short_url("rename");
    client.put(&url)
        .json(&serde_json::json!({
            "old_path": "/A",
            "new_path": "/B",
            "flags": 2 // EXCHANGE
        }))
        .send().await?.error_for_status()?;

    // ora file deve essere in B
    let res = client.get(&client.set_url("list", "/B"))
        .send().await?.error_for_status()?
        .json::<Vec<serde_json::Value>>().await?;

    assert_eq!(res.len(), 1);
    assert_eq!(res[0]["name"], "file.txt");

    // A deve esistere (era B) e essere vuota
    let res = client.get(&client.set_url("list", "/A"))
        .send().await?.error_for_status()?
        .json::<Vec<serde_json::Value>>().await?;
    assert!(res.is_empty());

    Ok(())
}

