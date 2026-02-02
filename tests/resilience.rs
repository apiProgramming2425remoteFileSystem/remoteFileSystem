use anyhow::{Result, anyhow};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

mod common;
use common::*;

// SIMPLE TCP PROXY HELPER
// This acts as a bridge: Client <-> Proxy <-> Server.
// We can "cut" the bridge using the `active` flag.
#[derive(Debug)]
struct IntermittentProxy {
    active: Arc<AtomicBool>,
    listening_port: u16,
}

impl IntermittentProxy {
    fn new(target_port: u16) -> Result<Self> {
        let listener = TcpListener::bind("localhost:0")?;
        let port = listener.local_addr()?.port();
        let active = Arc::new(AtomicBool::new(true));

        let proxy = Self {
            active: active.clone(),
            listening_port: port,
        };

        let active_clone = active.clone();
        thread::spawn(move || {
            for client in listener.incoming().flatten() {
                let active = active_clone.clone();
                thread::spawn(move || {
                    let _ = handle_proxy_conn(client, target_port, active);
                });
            }
        });

        Ok(proxy)
    }

    fn set_link_status(&self, is_up: bool) {
        self.active.store(is_up, Ordering::SeqCst);
    }
}

fn handle_proxy_conn(
    mut client: TcpStream,
    target_port: u16,
    active: Arc<AtomicBool>,
) -> Result<()> {
    // Link is down, close socket immediately.
    if !active.load(Ordering::SeqCst) {
        return Ok(());
    }

    // Connect to target server
    let mut server = TcpStream::connect(format!("localhost:{}", target_port))?;

    client.set_nonblocking(true)?;
    server.set_nonblocking(true)?;

    let mut buf = [0u8; 4096];

    loop {
        // If "Network is Down", during session, what should happen?
        if !active.load(Ordering::SeqCst) {
            // Close connections and exit
            break;

            // // This keeps sockets open but silent. Client needs Read-Timeouts to handle this.
            // thread::sleep(Duration::from_millis(100));
            // continue;
        }

        // Forward Client -> Server
        match client.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => server.write_all(&buf[..n])?,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        // Forward Server -> Client
        match server.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => client.write_all(&buf[..n])?,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        thread::sleep(Duration::from_millis(1)); // CPU yield
    }
    Ok(())
}

/// AUTO RECONNECT (The "Elevator" Test)
/// Simulates a temporary network drop. The client should hang/retry
/// and eventually succeed when the network returns.
#[test]
fn test_auto_reconnect_after_drop() -> Result<()> {
    // Setup Environment
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Start only the Server
    sys_build.no_client();
    let srv_ctx = sys_build.build()?;

    let server_port = srv_ctx
        .server
        .as_ref()
        .ok_or(anyhow!("Server not running"))?
        .port;

    // Start Proxy (Client connects to Proxy -> Proxy forwards to Server)
    let proxy = IntermittentProxy::new(server_port)?;

    // Start Client (Pointed at Proxy)
    sys_build = test_env.setup()?;
    sys_build.no_server();
    sys_build.port(proxy.listening_port);

    let clt_ctx = sys_build.build()?;

    let mount_point = clt_ctx
        .mount_point()
        .ok_or(anyhow!("Mount point not found"))?;
    let file_path = mount_point.join("resilience.txt");

    // Phase 1: Network UP
    fs::write(&file_path, "Before Drop\n")?;

    // Phase 2: Network DOWN (Simulate Cable Cut)
    println!("Cutting Network connection...");
    proxy.set_link_status(false);

    // Attempt Write (Should typically block or timeout depending on config)
    // For this test, we assume the client retries internally.
    // If your client is sync, this thread might block, so we spawn a writer.
    let file_path_clone = file_path.clone();
    let writer_handle = thread::spawn(move || {
        // This write attempts to happen during outage
        // It should eventually succeed once network is restored
        fs::write(file_path_clone, "During/After Drop\n").map_err(|e| e.to_string())
    });

    // Wait 2 seconds (simulating outage duration)
    thread::sleep(Duration::from_secs(2));

    // Phase 3: Network UP
    println!("Restoring Network connection...");
    proxy.set_link_status(true);

    // Wait for the blocked write to succeed
    match writer_handle.join() {
        Ok(Ok(())) => println!("Write succeeded after recovery"),
        Ok(Err(e)) => return Err(anyhow!("Write failed permanently: {}", e)),
        Err(e) => return Err(anyhow!("Writer thread panicked: {:?}", e)),
    }

    // Verify content (Last write wins or append, depending on logic. Here assume overwrite)
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, "During/After Drop\n");

    Ok(())
}

/// FAIL FAST ON HARD OUTAGE
/// If the network is down for too long, the OS should receive an I/O Error (EIO),
/// not hang forever.
#[test]
fn test_fail_fast_during_outage() -> Result<()> {
    // Setup Environment
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;

    // Start only the Server
    sys_build.no_client();
    let srv_ctx = sys_build.build()?;

    let server_port = srv_ctx
        .server
        .as_ref()
        .ok_or(anyhow!("Server not running"))?
        .port;

    // Start Proxy (Client connects to Proxy -> Proxy forwards to Server)
    let proxy = IntermittentProxy::new(server_port)?;

    // Start Client (Pointed at Proxy)
    sys_build = test_env.setup()?;
    sys_build.no_server();
    sys_build.port(proxy.listening_port);
    sys_build.client.arg("--no-cache");

    let clt_ctx = sys_build.build()?;

    let mount_point = clt_ctx
        .mount_point()
        .ok_or(anyhow!("Mount point not found"))?;
    let file_path = mount_point.join("fail_test.txt");

    let initial_content = "Initial Content\n";
    fs::write(&file_path, initial_content)?;
    let content = fs::read_to_string(&file_path)?;
    assert_eq!(content, initial_content);

    // Cut Network
    proxy.set_link_status(false);
    println!("Network cut. Attempting read...");

    // Attempt Read
    let start = std::time::Instant::now();
    let result = fs::read_to_string(&file_path);
    let duration = start.elapsed();

    // Assert Failure
    assert!(
        result.is_err(),
        "Read succeeded despite network being down?"
    );

    // Optional: Assert it didn't hang for default TCP timeout (60s)
    // This asserts your application has a reasonable request timeout (e.g., 5-10s)
    assert!(
        duration < Duration::from_secs(15),
        "Client hung for too long before failing"
    );

    Ok(())
}

/// SERVER RESTART RECOVERY
/// Kills the physical server process and spawns a new one.
/// The client should handle the "Connection Refused" and reconnect.
#[test]
fn test_server_process_restart() -> Result<()> {
    // Setup Environment
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;
    sys_build.client.arg("--no-cache");

    let mut ctx = sys_build.build()?;
    let mount_point = ctx.mount_point().ok_or(anyhow!("Mount point not found"))?;
    let file = mount_point.join("restart.txt");

    // Initial Success
    fs::write(&file, "Generation 1")?;

    // Kill Server
    println!("Killing Server...");
    ctx.stop_server();

    // Verify client fails or hangs momentarily
    let write_result = fs::write(&file, "Should Fail");
    assert!(
        write_result.is_err(),
        "Write succeeded despite server being down?"
    );

    // Start NEW Server (Same Port)
    println!("Respawning Server...");
    sys_build = test_env.setup()?;
    sys_build.no_client();
    sys_build.init_db(false); // Do not reinitialize DB

    let _srv_ctx = sys_build.build()?;

    // Allow Client to perform exponential backoff/reconnect
    thread::sleep(Duration::from_secs(1));

    // Verify Recovery
    // Write should succeed against the new server
    fs::write(&file, "Generation 2")?;

    let content = fs::read_to_string(&file)?;
    assert_eq!(content, "Generation 2");

    Ok(())
}
