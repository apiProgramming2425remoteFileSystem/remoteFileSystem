use std::time::Duration;
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

    tokio::time::sleep(Duration::from_millis(10)).await;

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

    tokio::time::sleep(Duration::from_millis(10)).await;

    // verifica xattr
    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    let res = client.get(&url).send().await?.error_for_status()?
        .json::<serde_json::Value>().await?;

    let returned_bytes = res["xattributes"].as_array().unwrap()
        .iter().map(|v| v.as_u64().unwrap() as u8).collect::<Vec<u8>>();
    assert_eq!(returned_bytes, b"hello");

    tokio::time::sleep(Duration::from_millis(10)).await;

    // delete xattr
    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    client.delete(&url).send().await?.error_for_status()?;

    let url = client.set_long_url("xattributes", "/dir_with_xattr", "names", Some("user.test"));
    let res = client.get(&url).send().await?;
    assert_eq!(res.status(), reqwest::StatusCode::NO_CONTENT);

    Ok(())
}

#[tokio::test]
async fn test_list_xattr_empty() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // mkdir
    let url = client.set_url("mkdir", "/dir");
    client.post(&url).send().await?.error_for_status()?;

    // list xattr
    let url = client.set_long_url("xattributes", "/dir", "names", None);
    let res = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    assert_eq!(res["names"].as_array().unwrap().len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_list_xattr_with_values() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = client.set_url("mkdir", "/dir");
    client.post(&url).send().await?.error_for_status()?;

    for name in ["user.a", "user.b"] {
        let url = client.set_long_url("xattributes", "/dir", "names", Some(name));
        client.put(&url)
            .json(&serde_json::json!({"xattributes": b"v".to_vec()}))
            .send().await?
            .error_for_status()?;
    }

    tokio::time::sleep(Duration::from_millis(10)).await;


    let url = client.set_long_url("xattributes", "/dir", "names", None);
    let res = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let mut names = res["names"]
        .as_array().unwrap()
        .iter().map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    names.sort();
    assert_eq!(names, vec!["user.a", "user.b"]);

    Ok(())
}


#[tokio::test]
async fn test_list_xattr_after_delete() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = client.set_url("mkdir", "/dir");
    client.post(&url).send().await?.error_for_status()?;

    let url = client.set_long_url("xattributes", "/dir", "names", Some("user.test"));
    client.put(&url)
        .json(&serde_json::json!({"xattributes": b"hello".to_vec()}))
        .send().await?
        .error_for_status()?;

    tokio::time::sleep(Duration::from_millis(10)).await;

    // delete
    client.delete(&url).send().await?.error_for_status()?;

    // list
    let url = client.set_long_url("xattributes", "/dir", "names", None);
    let res = client.get(&url)
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    assert!(res["names"].as_array().unwrap().is_empty());

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
    assert_eq!(attr.uid, common::TEST_USER_ID as u32);
    assert_eq!(attr.gid, common::TEST_GROUP_ID as u32);
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


#[tokio::test]
async fn attributes_after_delete_return_not_found() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = format!("{}?offset=0", client.set_url("files", "/a.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"data"))
        .send().await?
        .error_for_status()?;

    client.delete(&client.set_url("files", "/a.txt"))
        .send().await?
        .error_for_status()?;

    let resp = client.get(&client.set_url("attributes", "/a.txt"))
        .send().await?;

    assert_eq!(resp.status(), 404);
    Ok(())
}



#[tokio::test]
async fn xattrs_after_delete_are_gone() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    let url = format!("{}?offset=0", client.set_url("files", "/x.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"x"))
        .send().await?
        .error_for_status()?;

    tokio::time::sleep(Duration::from_millis(10)).await;


    let url = client.set_long_url("xattributes", "/x.txt", "names", Some("user.test"));
    client.put(&url)
        .json(&serde_json::json!({"xattributes": b"v".to_vec()}))
        .send().await?
        .error_for_status()?;

    tokio::time::sleep(Duration::from_millis(10)).await;


    client.delete(&client.set_url("files", "/x.txt"))
        .send().await?
        .error_for_status()?;

    tokio::time::sleep(Duration::from_millis(10)).await;


    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), 404);

    Ok(())
}



#[tokio::test]
async fn attributes_survive_rename() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // crea file
    let url = format!("{}?offset=0", client.set_url("files", "/old.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"hello"))
        .send().await?
        .error_for_status()?;

    let attr_before = client.get(&client.set_url("attributes", "/old.txt"))
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    // rename
    let url = client.set_short_url("rename");
    client.put(&url)
        .json(&serde_json::json!({
            "old_path": "/old.txt",
            "new_path": "/new.txt",
            "flags": 0
        }))
        .send().await?
        .error_for_status()?;

    // old path -> 404
    let resp = client.get(&client.set_url("attributes", "/old.txt")).send().await?;
    assert_eq!(resp.status(), 404);

    // new path -> stessi attributes (tranne maybe timestamps)
    let attr_after = client.get(&client.set_url("attributes", "/new.txt"))
        .send().await?
        .error_for_status()?
        .json::<Attributes>()
        .await?;

    assert_eq!(attr_after.size, attr_before.size);
    assert_eq!(attr_after.uid, attr_before.uid);
    assert_eq!(attr_after.gid, attr_before.gid);
    assert_eq!(attr_after.perm, attr_before.perm);

    Ok(())
}


#[tokio::test]
async fn xattrs_follow_rename() -> anyhow::Result<()> {
    let fs_root = tempfile::tempdir()?;
    let config = common::get_config(fs_root.path());
    let (client, _handle, _tmpdir) = common::start_server_app(config).await?;

    // crea file
    let url = format!("{}?offset=0", client.set_url("files", "/a.txt"));
    client.put(&url)
        .body(Bytes::from_static(b"x"))
        .send().await?
        .error_for_status()?;

    // set xattr
    let xurl_old = client.set_long_url("xattributes", "/a.txt", "names", Some("user.test"));
    client.put(&xurl_old)
        .json(&serde_json::json!({"xattributes": b"hello".to_vec()}))
        .send().await?
        .error_for_status()?;

    tokio::time::sleep(Duration::from_millis(10)).await;


    // rename
    let url = client.set_short_url("rename");
    client.put(&url)
        .json(&serde_json::json!({
            "old_path": "/a.txt",
            "new_path": "/b.txt",
            "flags": 0
        }))
        .send().await?
        .error_for_status()?;

    // old xattr -> 404
    let resp = client.get(&xurl_old).send().await?;
    assert_eq!(resp.status(), 404);

    // new xattr -> presente
    let xurl_new = client.set_long_url("xattributes", "/b.txt", "names", Some("user.test"));
    let res = client.get(&xurl_new)
        .send().await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    tokio::time::sleep(Duration::from_millis(10)).await;


    let bytes = res["xattributes"]
        .as_array().unwrap()
        .iter().map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<u8>>();

    assert_eq!(bytes, b"hello");
    Ok(())
}
