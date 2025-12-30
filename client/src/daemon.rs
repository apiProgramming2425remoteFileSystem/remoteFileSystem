use std::sync::Arc;

use crate::error::{DaemonError, RfsClientError};

use anyhow;
use async_trait::async_trait;
use tokio::sync::Notify;
use tracing::{Level, instrument};

type Result<T> = std::result::Result<T, DaemonError>;

/// Daemon representation
#[derive(Clone)]
pub struct Daemon {
    shutdown_notify: Arc<Notify>,
    foreground: bool,
}

/// Trait for daemon service operations
#[async_trait]
pub trait DaemonService {
    /// Starts the service, performing necessary setup.
    fn start(&self) -> Result<()> {
        Err(DaemonError::UnsupportedPlatform(
            "Daemon start not supported on this platform".into(),
        ))
    }

    /// Signal handler for graceful shutdown.
    async fn signal_handler(&self) {}
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            shutdown_notify: Arc::new(Notify::new()),
            foreground: false,
        }
    }

    pub fn foreground(mut self, foreground: bool) -> Self {
        self.foreground = foreground;
        self
    }

    /// Initialize and start the daemon
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub fn initialize(&self) -> Result<()> {
        println!("Initializing RemoteFS Daemon...");

        // Create background daemon process
        self.start()?;

        Ok(())
    }

    /// Run the daemon with the provided future
    /// **IMPORTANT**: This function needs to be called after daemonizing the process
    #[instrument(skip(self, future), err(level = Level::ERROR))]
    pub fn create_runtime<F>(&self, future: F) -> Result<()>
    where
        F: Future<Output = std::result::Result<(), RfsClientError>> + Send + 'static,
    {
        // Spawn the future in the tokio runtime
        // Important: we build the runtime here to ensure it's created after demonizing
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                DaemonError::Other(anyhow::format_err!(
                    "Failed to build Tokio runtime: {}",
                    err
                ))
            })?;

        tracing::info!("Async Runtime started. Preparing Remote File System...");

        runtime.block_on(async {
            // Spawn the signal handler (Kill/Ctrl+C)
            self.spawn_signal_handler();

            // Execute the main future (run_async passed from lib.rs)
            if let Err(e) = future.await {
                eprintln!("Runtime error: {}", e);
                tracing::error!("Runtime error: {}", e);
            }
        });

        Ok(())
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub async fn wait_for_shutdown(&self) {
        self.shutdown_notify.notified().await;
    }

    fn spawn_signal_handler(&self) {
        let daemon = self.clone();

        tokio::spawn(async move {
            daemon.signal_handler().await;
            tracing::info!("Signal received, notifying shutdown...");
            daemon.shutdown_notify.notify_waiters();
        });
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use daemonize::{Daemonize, Outcome, Parent};
    use std::fs;
    use std::process;

    #[async_trait]
    impl DaemonService for Daemon {
        fn start(&self) -> Result<()> {
            // Determine if we should daemonize or run in foreground
            if self.foreground {
                println!("Running in foreground mode, not daemonizing.");
                tracing::info!("Running in foreground mode, not daemonizing.");
                return Ok(());
            }

            let stdout = fs::File::create("/tmp/remote-fs.out").unwrap();
            let stderr = fs::File::create("/tmp/remote-fs.err").unwrap();

            let daemon = Daemonize::new()
                .pid_file("/tmp/remote-fs.pid")
                .chown_pid_file(true)
                .working_directory("/")
                .stdout(stdout) // Redirect stdout to `/tmp/remote-fs.out`.
                .stderr(stderr); // Redirect stderr to `/tmp/remote-fs.err`.
            // Start the daemon process
            match daemon.execute() {
                Outcome::Parent(Ok(Parent {
                    first_child_exit_code,
                    ..
                })) => {
                    // The parent exits here, freeing the shell
                    println!("Service started in background (Logs in /tmp/remote-fs.*)");
                    process::exit(first_child_exit_code)
                }
                Outcome::Child(Ok(_child)) => {
                    // Child continues here
                    println!("Daemon started successfully.");
                    // Install a panic hook to capture unexpected crashes in the .err file
                    std::panic::set_hook(Box::new(|info| {
                        eprintln!("PANIC: {:?}", info);
                    }));
                    Ok(())
                }
                Outcome::Child(Err(err)) | Outcome::Parent(Err(err)) => {
                    eprintln!("Error during daemonize: {}", err);
                    Err(DaemonError::StartFailed(err.to_string()))
                }
            }
        }

        /// Handles Unix signals for graceful shutdown (SIGTERM, SIGINT, SIGHUP)
        async fn signal_handler(&self) {
            use tokio::signal::unix::{SignalKind, signal};

            let mut interrupt = signal(SignalKind::interrupt()).expect("Failed to bind SIGINT");
            let mut terminate = signal(SignalKind::terminate()).expect("Failed to bind SIGTERM");
            let mut quit = signal(SignalKind::quit()).expect("Failed to bind SIGQUIT");
            let mut sighup = signal(SignalKind::hangup()).expect("Failed to bind SIGHUP");

            tokio::select! {
                _ = interrupt.recv() => {
                    tracing::info!("SIGINT (Ctrl+C) received, shutting down daemon...");
                },
                _ = terminate.recv() => {
                    tracing::info!("SIGTERM received, shutting down daemon...");
                },
                _ = quit.recv() => {
                    tracing::info!("SIGQUIT received, shutting down daemon...");
                },
                _ = sighup.recv() => {
                    tracing::info!("SIGHUP received, shutting down daemon...");
                },
            }
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;

    #[async_trait]
    impl DaemonService for Daemon {}
}
