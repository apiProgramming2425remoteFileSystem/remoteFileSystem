use anyhow::Result;
use remote_fs_core::SystemBuilder;
use std::ffi::{OsStr, OsString};
use std::fmt::Display;
use std::process::Command;
use tempfile::TempDir;

pub const SERVER_HOST: &str = "localhost";
pub const SERVER_PORT: u16 = 8080;
pub const MOUNT_DIR_NAME: &str = "mnt_e2e_tests";
pub const SERVER_ROOT_NAME: &str = "srv_e2e_tests";
pub const TEST_LOGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/logs/e2e_tests");

/// Macro to handle TestEnvironment setup boilerplate.
/// Returns a tuple: (ctx, mount_point, server_root)
#[macro_export]
macro_rules! setup_e2e {
    () => {{
        let test_env = TestEnvironment::new()?;
        let sys_build = test_env.setup()?;
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

        (ctx, mount_point, server_root)
    }};
}

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

/// Runs `binary args... target`
/// Returns (client_output, server_output)
pub fn compare_command_outputs<I, A, S, T>(
    binary: S,
    args: I,
    client_target: T,
    server_target: T,
) -> Result<(String, String)>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
    S: AsRef<OsStr> + Display,
    T: AsRef<OsStr>,
{
    let bin = binary.as_ref();

    // Collect args once to avoid consuming the iterator multiple times
    let args_vec: Vec<OsString> = args
        .into_iter()
        .map(|a| a.as_ref().to_os_string())
        .collect();

    let args_display = args_vec
        .iter()
        .map(|s| s.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ");

    println!(
        "Command: {} {} {}",
        bin.to_string_lossy(),
        args_display,
        client_target.as_ref().to_string_lossy(),
    );

    // Run on Server
    let s_out = Command::new(bin)
        .args(&args_vec)
        .arg(server_target.as_ref())
        .output()?;
    assert!(
        s_out.status.success(),
        "Server command failed: {} {} {}",
        bin.to_string_lossy(),
        args_display,
        server_target.as_ref().to_str().unwrap_or("<invalid utf8>")
    );

    let s_str = String::from_utf8_lossy(&s_out.stdout).to_string();

    println!("Server Output:\n{}", s_str);

    // Run on Client
    let c_out = Command::new(bin)
        .args(&args_vec)
        .arg(client_target.as_ref())
        .output()?;
    assert!(
        c_out.status.success(),
        "Client command failed: {} {} {}",
        bin.to_string_lossy(),
        args_display,
        client_target.as_ref().to_str().unwrap_or("<invalid utf8>")
    );

    let c_str = String::from_utf8_lossy(&c_out.stdout).to_string();

    println!("Client Output:\n{}", c_str);

    Ok((c_str, s_str))
}
