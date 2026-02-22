# Testing Guide

This project uses a robust testing strategy combining **Integration**, and **End-to-End (E2E)** tests. We use modern Rust tooling (`cargo-nextest`, `cargo-llvm-cov`) to provide a fast, Jest-like developer experience.

## Prerequisites

Before running the tests, ensure you have the necessary system libraries (for FUSE) and Cargo tools installed.

### 1. System Dependencies

You need FUSE headers installed on your machine.

- **Ubuntu/Debian:** `sudo apt install libfuse3-dev fuse3`
- **Fedora:** `sudo dnf install fuse3-devel`
<!-- - **macOS:** Requires [macFUSE](https://osxfuse.github.io/). -->

### 2. Rust Tools

We use specific tools for speed and coverage. Install them globally:

```bash
cargo install --locked cargo-nextest
cargo install cargo-llvm-cov
```

---

## Testing Guide & Architecture

This project uses a robust testing pyramid strategy: Integration (component interaction), and End-to-End (real binary execution).


1. **Integration Tests (User Flows)**

    Integration tests verify how major components (Client Daemon, FUSE Interface, Server API) work together.

    **Client Structure** (`client/tests/`) Tests the interaction: `Kernel (FUSE) ↔ Client ↔ Mock Network`.

    - `read_flow.rs`: `cat`, `open`, sequential vs random read, cache hits.
    - `write_flow.rs`: `cp`, `echo`, write buffer flushing, upload behavior.
    - `metadata_flow.rs`: `chmod`, `chown`, `stat`, extended attributes.
    <!-- - `resilience_flow.rs`: Recovery from Server 500s, timeouts, and network drops. -->
    - `auth_flow.rs`: Login sequences, token refresh.
    - `cli_parsing.rs`: Argument parsing verification.
    - `common/`: Shared setup (Mock factories, wait_for_mount helpers).

    **Server Structure** (`server/tests/`) Tests the interaction: `HTTP Client ↔ Actix Server ↔ DB/Storage`.

    - `auth_flow.rs`: Middleware validation, JWT logic.
    - `read_flow.rs`: Download endpoints.
    - `write_flow.rs`: Upload endpoints.
    - `metadata_flow.rs`: Stats, permission and attributes endpoints.
    - `common/`: Helpers for random port binding and DB setup.

2. **End-to-End (E2E) Tests (Real Binaries)**

    E2E tests treat the application as a Black Box. They compile the actual binaries, spawn them as real OS processes, and execute shell commands against the mounted filesystem.

    **Location**: `tests/` (workspace root).

    - `basic_ops.rs`: Standard mkdir, touch, rm.
    - `edge_cases.rs`: Large files, deep nesting.
    <!-- - `src/:` Custom test infrastructure (Binary builders, Process managers). -->

---

## Client Integration Strategy (FUSE)

Testing the Client is critical because it involves complex interactions: **Kernel (FUSE) ↔ Client App ↔ Network (Mock)**.

#### The Test Lifecycle

Every integration test strictly follows this order to prevent Deadlocks or Race Conditions:

1. **Mock Setup**: Define network expectations (e.g., "If file X is requested, return Y") using `mockall`.
2. **App Start**: Launch the client app in a background task (`tokio::spawn`).
3. **Wait for Mount: CRITICAL**. The test must wait for the filesystem to be mounted and ready before proceeding.
4. **Execution**: Perform file operations using `tokio::fs` (avoid `std::fs` to prevent blocking the async runtime).
5. **Shutdown Trigger**: Request a graceful shutdown.
6. **Cleanup Wait**: Await the background task to ensure a clean exit.

#### Implementation Pattern

``` rust
#[tokio::test]
async fn test_example_flow() -> Result<()> {
    // 1. Setup Mock
    let mut mock = MockRemoteStorage::new();
    mock.expect_get_metadata().returning(|_| Ok(Default::default()));

    // 2. Start App (Returns daemon handle and Log Guard)
    // Note: Keep the Log Guard alive to capture logs during the test.
    // 3. Wait for Mount (Robust polling), done internally of `start_client_app`
    let app_controller = common::AppController::start(config, mock).await?;

    // 4. Execution
    // Use `run_with_timeout` to ensure the app doesn't crash.
    // Use tokio::fs to perform file operations on the mounted filesystem, because it async and non-blocking.
    let meta = app_controller.run_with_timeout(tokio::fs::metadata(mount_point.join("file"))).await??;
    assert!(meta.is_file());

    // 5. Shutdown (Uses notify_one to avoid races)
    // 6. Cleanup
    app_controller.shutdown().await?;
    Ok(())
}
```

---

## Server Integration Strategy (Actix & TCP)

Testing the Server involves verifying real HTTP APIs. The main challenge is running tests in parallel without port conflicts.

#### The "Random Port Binding" Pattern

We never use the fixed port 8080 in this tests. We bind to port 0, asking the OS for a free ephemeral port.

#### The Test Lifecycle

1. **Bind Async**: The test binds to `localhost:0` using `tokio::net::TcpListener`.
2. **Get Port**: We retrieve the actual port number assigned by the OS.
3. **Convert & Pass**: We convert the listener to `std::net::TcpListener` and pass it to Actix.
4. **Check Connection**: The test client attempts to connect in a loop until the server responds or a timeout.
5. **Execution**: Perform HTTP requests.

#### Implementation Pattern

``` rust
#[tokio::test]
async fn test_server_startup() -> Result<()> {
    // 1. Bind to port 0 (Explicit IPv4)
    // 2. Configure Client and Start App
    // 3. Pass the listener (converted to std) to the app
    // 4. Check Connection
    let (_log, http_client, app_handle) = common::start_server_app(config).await?;

    // 5. Execution
    // Real HTTP Request
    let resp = client.get("/health").send().await?;
    assert_eq!(resp.status(), 200);

    Ok(())
}
```

---

## End-to-End (E2E) Infrastructure

E2E tests treat the application as a **Black Box**. They compile the actual binaries, spawn them as real processes, and execute shell commands against the mounted filesystem.

#### E2E Workflow

The `src/` modules provide the infrastructure to:

1. **Build**: Automatically invokes `cargo build` via `escargot` with caching.
2. **Spawn**: Launches Server and Client binaries as child process.
3. **Execute**: Run standard linux commands (`ls`, `cp`, `diff`).
4. **Verify**: Check exit codes and file integrity.
5. **Kill**: Send `SIGINT/SIGTERM` to clean up processes.

### Example E2E Test

``` rust
// tests/e2e.rs
#[test]
fn test_end_to_end_upload() -> Result<()> {
    // Setup system using the Builder pattern
    let test_env = TestEnvironment::new()?;
    let sys_build = test_env.setup()?;


    // Build the context (compiles binaries, spawns server, spawns client, waits for mount)
    let ctx = sys_build.build()?;

    let Some(mount_dir) = ctx.mount_point() else {
        return Err(anyhow!("Client context missing"));
    };

    // Execute Shell Commands
    let status = Command::new("touch")
        .arg(mount_dir.join("file.txt"))
        .status()?;
    
    assert!(status.success());

    // Verification: Check the server-side storage directly
    let Some(server) = &ctx.server else {
        return Err(anyhow!("Server root missing"));
    };

    assert!(server.fs_root.join("file.txt").exists());

    // Cleanup 
    println!("Test completed, resources will be cleaned up on drop.");
    Ok(())
}
```

#### E2E Helpers

The repository provides small E2E helpers in `tests/common/mod.rs` to keep test code concise. Use them for simple tests; prefer manual setup when you need custom client/server configuration.

- `setup_e2e!()`
  - Quick default setup. Builds a temporary `TestEnvironment`, calls `setup()` and `build()` on the `SystemBuilder`, and returns `(ctx, mount_point, server_root)` where `mount_point` and `server_root` are owned `PathBuf`s ready for use.

    ```rust
    let (ctx, mount_point, server_root) = setup_e2e!();
    std::fs::write(mount_point.join("file.txt"), "content")?;
    ```

  - The macro uses the default `SystemBuilder` configuration. If a test needs additional customization, build the setup manually:

    ```rust
    let test_env = TestEnvironment::new()?;
    let mut sys_build = test_env.setup()?;
    sys_build.client.arg_pair("--cache-ttl", "1");
    // other sys_build customization...
    let ctx = sys_build.build()?;
    ```

    Use `setup_e2e!()` for straightforward E2E checks and manual `TestEnvironment` + `SystemBuilder` for any test requiring non-default behavior.

- `compare_command_outputs(binary, args, client_target, server_target)`
  - Run the same command against the client mount and server storage, assert both succeed, and return their stdout for comparison (e.g., `md5sum`, `ls`).

    ```rust
    let (client_out, server_out) = compare_command_outputs(
        "md5sum",
        ["-b"],
        &mount_point.join("file.bin"),
        &server_root.join("file.bin"),
    )?;
    assert_eq!(client_out, server_out);
    ```


---

## Quick Start & Aliases

The project includes pre-configured aliases in `.cargo/config.toml` to streamline testing.

| Base Command    | Scope                                     | Profile     |
| --------------- | ----------------------------------------- | ----------- |
| cargo test-all  | All tests (Unit + Integration + E2E).     | default     |
| cargo test-unit | Unit tests only (lib & bin targets).      | unit        |
| cargo test-int  | Integration tests only (excludes E2E).    | integration |
| cargo test-e2e  | E2E tests only (with retries & timeouts). | e2e         |

#### Command Variants

Each base command has corresponding variants for specific needs:

- Debug (`-d`): Runs with `1` thread, immediate output, long timeouts, and `--no-capture` to show logs in real-time.
  - *Example:* `cargo test-e2e-d`
- List (`-list`): Lists available tests without executing them.
  - Example: cargo test-list-all

#### Code Coverage

To measure code coverage, use the `-cov` variant or generate a visual report.

- **Run with Coverage**: Append `-cov` to any base command to run with LLVM instrumentation (e.g., `cargo test-unit-cov`).

- **Generate HTML Report**: Run `cargo test-all-cov-html` to execute all tests and immediately open the detailed HTML report in your browser.
