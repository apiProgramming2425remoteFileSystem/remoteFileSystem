use anyhow::Result;
use remote_fs_core::SystemBuilder;
use tempfile::TempDir;

pub const SERVER_HOST: &str = "localhost";
pub const SERVER_PORT: u16 = 8080;
pub const MOUNT_DIR_NAME: &str = "mnt_e2e_tests";
pub const SERVER_ROOT_NAME: &str = "srv_e2e_tests";
pub const TEST_LOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/logs/e2e_tests");

pub fn setup() -> Result<SystemBuilder> {
    // Setup Environment
    let temp_dir = TempDir::new()?;
    let mount_dir = temp_dir.path().join(MOUNT_DIR_NAME);
    let server_root = temp_dir.path().join(SERVER_ROOT_NAME);

    // setup code specific to your library's tests would go here
    Ok(SystemBuilder::new(
        SERVER_HOST,
        SERVER_PORT,
        server_root.as_path(),
        mount_dir.as_path(),
    ))
}
