use serde::{Deserialize, Serialize};
use bytes::Bytes;
use crate::common::{Attributes, FileType, SetAttr, SetAttrRequest};

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


#[tokio::test]
async fn test_get_attributes_new_directory() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // mkdir
    let url = client.set_url("mkdir", "/dir");
    client.post(&url).send().await?.error_for_status()?;

    // get attributes
    let url = client.set_url("attributes", "/dir");
    let attr = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert_eq!(attr.kind, FileType::Directory);
    assert_eq!(attr.uid, (common::TEST_USER_ID + 1000) as u32);
    assert_eq!(attr.gid, (common::TEST_GROUP_ID + 1000) as u32);
    assert!(attr.nlink >= 1);

    Ok(())
}

#[tokio::test]
async fn test_get_attributes_new_file() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // write file
    let url = format!("{}?offset=0", client.set_url("files", "/file.txt"));
    client.put(&url)
        .body(Bytes::from("hello world"))
        .send().await?
        .error_for_status()?;

    // get attributes
    let url = client.set_url("attributes", "/file.txt");
    let attr = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert_eq!(attr.kind, FileType::RegularFile);
    assert_eq!(attr.size, 11);

    Ok(())
}

#[tokio::test]
async fn test_write_updates_mtime_and_ctime() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // first write
    let url = format!("{}?offset=0", client.set_url("files", "/file.txt"));
    client.put(&url)
        .body(Bytes::from("abc"))
        .send().await?
        .error_for_status()?;

    let url_attr = client.set_url("attributes", "/file.txt");
    let attr1 = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    // second write
    client.put(&url)
        .body(Bytes::from("abcdef"))
        .send().await?
        .error_for_status()?;

    let attr2 = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert!(attr2.mtime > attr1.mtime);
    assert!(attr2.ctime >= attr1.ctime);
    assert!(attr2.atime >= attr1.atime);

    Ok(())
}


#[tokio::test]
async fn test_set_attributes_changes_only_mode() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // create file
    let url = format!("{}?offset=0", client.set_url("files", "/file.txt"));
    client.put(&url)
        .body(Bytes::from("data"))
        .send().await?
        .error_for_status()?;

    let url_attr = client.set_url("attributes", "/file.txt");
    let before = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;


    let request = SetAttrRequest::new(SetAttr {
        mode: Some(0o600 as u32),
        ..Default::default()
    },);
    // set mode
    client.put(&url_attr)
        .json(&request)
        .send().await?
        .error_for_status()?;

    let after = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert_eq!(after.perm & 0o777, 0o600);
    assert_eq!(after.uid, before.uid);
    assert_eq!(after.gid, before.gid);
    assert_eq!(after.size, before.size);

    Ok(())
}



#[tokio::test]
async fn test_set_attributes_truncate_file() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // create file
    let url = format!("{}?offset=0", client.set_url("files", "/file.txt"));
    client.put(&url)
        .body(Bytes::from("hello world"))
        .send().await?
        .error_for_status()?;

    let url_attr = client.set_url("attributes", "/file.txt");
    let before = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    // truncate
    let request = SetAttrRequest::new(SetAttr {
        size: Some(5),
        ..Default::default()
    },);
    client.put(&url_attr)
        .json(&request)
        .send().await?
        .error_for_status()?;

    let after = client.get(&url_attr)
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert_eq!(after.size, 5);
    assert!(after.mtime >= before.mtime);
    assert!(after.ctime >= before.ctime);

    Ok(())
}


