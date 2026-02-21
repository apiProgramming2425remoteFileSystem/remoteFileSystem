use anyhow::Result;
#[cfg(unix)]
use libc;
use std::fs;
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

mod common;
use common::*;

#[cfg(unix)]
#[test]
fn test_server_error_mapping() -> Result<()> {
    let (_ctx, mount_root, server_root) = setup_e2e!();

    // Create file on Server directly
    let secret_file = server_root.join("secret.data");
    fs::write(&secret_file, "top secret")?;

    // Remove all permissions on the SERVER copy
    // The server process (running as your user) will now fail to open this.
    let mut perms = fs::metadata(&secret_file)?.permissions();
    perms.set_mode(0o000); // No Read, No Write
    fs::set_permissions(&secret_file, perms.clone())?;

    // Attempt Read via Client
    // Client sends GET -> Server tries open() -> Fails EACCES -> Returns 403 -> Client returns EACCES
    let client_file = mount_root.join("secret.data");
    let result = fs::read_to_string(&client_file);

    // Verify accurate error mapping
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Rust maps libc::EACCES to PermissionDenied
    assert_eq!(
        err.kind(),
        ErrorKind::PermissionDenied,
        "Client returned generic error {:?} instead of PermissionDenied",
        err.kind()
    );

    Ok(())
}

/// NOT FOUND (ENOENT)
/// Verifies that accessing a missing file returns NotFound, not generic Error.
#[test]
fn test_error_mapping_enoent() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let missing_path = mount_point.join("ghost.txt");

    // Attempt Open
    let result = fs::File::open(&missing_path);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);

    Ok(())
}

/// ALREADY EXISTS (EEXIST)
/// Verifies that `mkdir` on an existing directory fails correctly.
/// Critical for `mkdir -p` to work (it ignores EEXIST but fails on other errors).
#[test]
fn test_error_mapping_eexist() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let dir = mount_point.join("my_dir");
    fs::create_dir(&dir)?;

    // Attempt Create Duplicate
    let result = fs::create_dir(&dir);

    assert!(result.is_err());
    let err = result.unwrap_err();

    assert_eq!(err.kind(), ErrorKind::AlreadyExists);

    Ok(())
}

/// NOT A DIRECTORY (ENOTDIR)
/// Verifies that trying to use a file as a directory fails.
/// e.g. `cd file.txt/child`
#[cfg(unix)]
#[test]
fn test_error_mapping_enotdir() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let file = mount_point.join("file.txt");
    fs::write(&file, "content")?;

    // Attempt to treat file as parent dir
    // Path: /mnt/file.txt/child
    let invalid_path = file.join("child");
    let result = fs::File::open(&invalid_path);

    assert!(result.is_err());

    // Rust maps ENOTDIR to NotADirectory (since 1.46+) or broadly invalid input
    // We check the raw OS error if possible, or standard ErrorKind
    let err = result.unwrap_err();

    // Note: Some OSs might return NotFound if the path logic is handled in FUSE lookup.
    // However, correct POSIX behavior for "component exists but is not dir" is ENOTDIR.
    // If your client handles lookups component-by-component, it should catch this.
    // We accept both for robustness, but prefer NotADirectory.
    assert!(
        err.kind() == ErrorKind::NotADirectory || err.raw_os_error() == Some(libc::ENOTDIR),
        "Expected ENOTDIR, got {:?}",
        err
    );

    Ok(())
}

/// IS A DIRECTORY (EISDIR)
/// Verifies that trying to `read` a directory like a file fails.
/// e.g. `cat my_dir`
#[cfg(unix)]
#[test]
fn test_error_mapping_eisdir() -> Result<()> {
    let (_ctx, mount_point, _server_root) = setup_e2e!();

    let dir = mount_point.join("folder");
    fs::create_dir(&dir)?;

    // Attempt to Open directory as file (read mode)
    // Note: OpenOptions read(true) implies O_RDONLY
    let result = fs::read_to_string(&dir);

    assert!(result.is_err());

    // On Linux, open() on a dir with O_RDONLY succeeds, but read() fails with EISDIR.
    // However, std::fs::read_to_string handles the open/read sequence.
    // Let's check strict standard library behavior.
    let err = result.unwrap_err();

    // Rust often maps this to a generic OS error or specific IsADirectory (experimental).
    // Raw OS error 21 is EISDIR on Linux.
    assert_eq!(
        err.raw_os_error(),
        Some(libc::EISDIR),
        "Expected EISDIR (21), got {:?}",
        err
    );

    Ok(())
}

/// PERMISSION DENIED (EACCES)
/// Verifies that the client enforces permission bits locally or propagates 403.
#[cfg(unix)]
#[test]
fn test_error_mapping_eacces() -> Result<()> {
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    sys_build.client.arg("--no-cache");

    let ctx = sys_build.build()?;

    // We clone paths to own them outside the ctx borrow lifetime if needed
    let mount_point = ctx
        .mount_point()
        .ok_or_else(|| anyhow::anyhow!("Client mount point missing"))?
        .to_path_buf();

    let server_root = ctx
        .server_root()
        .ok_or_else(|| anyhow::anyhow!("Server root missing"))?
        .to_path_buf();

    // let (_ctx, mount_point, _server_root) = setup_e2e!();

    let file = mount_point.join("secret.txt");
    fs::write(&file, "top secret")?;

    // Set 000 permissions (No access)
    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&file, perms)?;

    // Attempt Read
    let result = fs::read_to_string(&file);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);

    Ok(())
}
