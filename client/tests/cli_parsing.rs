use anyhow::Result;
use clap::Parser;
use std::path::Path;

use client::config::*;

#[test]
fn test_cli_args_override() -> Result<()> {
    let args = RfsCliArgs::parse_from([
        "test_binary",
        "--mount-point",
        "/tmp/cli_mount",
        "--server-url",
        "http://cli-server",
        "--log-targets",
        "file",
    ]);
    let config = RfsConfig::load(&args)?;
    assert_eq!(config.mount_point, Path::new("/tmp/cli_mount"));
    assert_eq!(config.server_url, "http://cli-server");
    assert!(config.logging.log_targets.contains(&LogTargets::File));
    Ok(())
}

#[test]
fn test_env_override() -> Result<()> {
    temp_env::with_vars(
        [
            ("RFS__MOUNT_POINT", Some("/tmp/mount")),
            ("RFS__SERVER_URL", Some("http://env-server")),
            ("RFS__LOGGING__LOG_TARGETS", Some("console")),
        ],
        || {
            let args = RfsCliArgs::parse_from(["test_binary"]);
            let config = RfsConfig::load(&args)?;
            assert_eq!(config.mount_point, Path::new("/tmp/mount"));
            assert_eq!(config.server_url, "http://env-server");
            assert!(config.logging.log_targets.contains(&LogTargets::Console));
            Ok(())
        },
    )
}

#[test]
fn test_toml_config_override() -> Result<()> {
    // Create a temporary TOML config file
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
        mount_point = "/tmp/toml_mount"
        server_url = "http://toml-server"

        [logging]
        log_targets = ["console", "file"]
        "#,
    )?;

    let args = RfsCliArgs::parse_from([
        "test_binary",
        "--config-file",
        config_path.to_str().unwrap(),
    ]);
    let config = RfsConfig::load(&args)?;
    assert_eq!(config.mount_point, Path::new("/tmp/toml_mount"));
    assert_eq!(config.server_url, "http://toml-server");
    assert!(config.logging.log_targets.contains(&LogTargets::Console));
    assert!(config.logging.log_targets.contains(&LogTargets::File));
    Ok(())
}

#[test]
fn test_default_values() -> Result<()> {
    let args = RfsCliArgs::parse_from(["test_binary"]);
    let config = RfsConfig::load(&args)?;
    assert_eq!(config.mount_point, Path::new("/mnt/remote-fs"));
    assert_eq!(config.server_url, "http://localhost:8080");
    assert!(config.logging.log_targets.contains(&LogTargets::All));
    Ok(())
}

#[test]
fn test_invalid_log_target() {
    let args = RfsCliArgs::try_parse_from(["test_binary", "--log-targets", "invalid_target"]);
    assert!(args.is_err());
}
