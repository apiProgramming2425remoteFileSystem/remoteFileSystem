use anyhow::Result;
use remote_fs_core::SystemBuilder;
use tempfile::TempDir;

pub const SERVER_HOST: &str = "localhost";
pub const SERVER_PORT: u16 = 8080;
pub const MOUNT_DIR_NAME: &str = "mnt_e2e_tests";
pub const SERVER_ROOT_NAME: &str = "srv_e2e_tests";
pub const TEST_LOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/logs/e2e_tests");

pub struct TestEnvironment {
    /// Temporary directory holding all test files
    temp_dir: TempDir,
}

impl TestEnvironment {
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        println!("Created temporary test directory at: {:?}", temp_dir.path());
        Ok(Self { temp_dir })
    }

    pub fn setup(&self) -> Result<SystemBuilder> {
        // Setup Environment
        let mount_dir = self.temp_dir.path().join(MOUNT_DIR_NAME);
        let server_root = self.temp_dir.path().join(SERVER_ROOT_NAME);
        let db_path = self.temp_dir.path().join("e2e_test_db.sqlite");

        // setup code specific to your library's tests would go here
        Ok(SystemBuilder::new(
            SERVER_HOST,
            SERVER_PORT,
            server_root.as_path(),
            db_path.as_path(),
            mount_dir.as_path(),
        ))
    }
}
