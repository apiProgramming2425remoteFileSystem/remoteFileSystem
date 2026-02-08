slint::slint! {
    import { App, ViewState } from "src/gui/app.slint";
    export { App, ViewState }
}

use std::{path::PathBuf, sync::Arc};

use crate::mount::MountOptions;
use crate::{
    config::RfsConfig, daemon::Daemon, error::GUIError, network::RemoteStorage, run_async,
};
use slint::{SharedString, Weak};
use tokio::{runtime::Runtime, sync::Mutex};

pub struct Gui<R: RemoteStorage + Clone> {
    ui: App,
    rc: R,
    rt: Arc<Runtime>,
    daemon: Arc<Mutex<Option<Daemon>>>,
    default_config: RfsConfig,
    config: Arc<Mutex<RfsConfig>>,
}

/* UTIL FUNCTIONS */
async fn health_check<R: RemoteStorage + Clone>(
    rc: R,
    ui_weak: Arc<Weak<App>>,
    rt: Arc<Runtime>,
) -> bool {
    let rc_thread = rc.clone();
    let ui_thread = ui_weak.clone();

    let handle = rt.spawn(async move {
        // 1. asyncronous call
        let result = rc_thread.health_check().await;

        let is_ok = result.is_ok();

        // 2. result management and ui upgrade
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_thread.upgrade() {
                match result {
                    Ok(_) => {
                        tracing::info!("Server is available!");
                        ui.set_server_unavailable(false);
                        ui.set_is_loading(false);
                    }
                    Err(err) => {
                        tracing::error!("Server error: {err}");
                        ui.set_server_unavailable(true);
                        ui.set_is_loading(false);
                    }
                }
            }
        });

        is_ok
    });

    handle.await.unwrap()
}

fn load_config(ui: &App, config_: Arc<Mutex<RfsConfig>>) -> Result<(), GUIError> {
    let status_config = config_.blocking_lock();
    let config = &status_config;

    let log_dir = if let Some(dir) = &config.logging.log_dir {
        dir.to_string_lossy().into_owned()
    } else {
        String::from("")
    };

    let log_file = if let Some(file) = &config.logging.log_file {
        file.to_string_lossy().into_owned()
    } else {
        String::from("")
    };

    let log_rotation = if let Some(rotation) = &config.logging.log_rotation {
        rotation.to_string()
    } else {
        String::from("")
    };

    /* CONFIG MANAGEMENT */
    ui.set_mount_settings(MountSettings {
        allow_other: config.mount.allow_other,
        mount_point: SharedString::from(config.mount_point.to_string_lossy().into_owned()),
        privileged: config.mount.privileged,
        read_only: config.mount.read_only,
    });

    /* CACHE MANAGEMENT */
    ui.set_cache_settings(CacheSettings {
        capacity: config.cache.capacity as i32,
        enabled: config.cache.enabled,
        max_size: config.cache.max_size as i32,
        policy: SharedString::from(config.cache.policy.to_string()),
        ttl: config.cache.ttl as i32,
        use_ttl: config.cache.use_ttl,
    });

    /* FS AND LOGGING MANAGEMENT */
    ui.set_fs_settings(FsSettings {
        buffer_size: config.file_system.buffer_size as i32,
        foreground: config.foreground,
        max_pages: config.file_system.max_pages as i32,
        page_size: config.file_system.page_size as i32,
        xattr_enable: config.file_system.xattr_enable,
    });
    ui.set_log_settings(LogSettings {
        dir: SharedString::from(log_dir),
        file: SharedString::from(log_file),
        format: SharedString::from(config.logging.log_format.to_string()),
        level: SharedString::from(config.logging.log_level.to_string_gui()),
        rotation: SharedString::from(log_rotation),
    });

    Ok(())
}

fn unmount(daemon: Arc<Mutex<Option<Daemon>>>) {
    let mut lock = daemon.blocking_lock();
    if let Some(daemon) = lock.take() {
        tracing::info!("Unmounting RemoteFS...");
        daemon.trigger_shutdown();
        tracing::info!("RemoteFS correctly unmounted.");
    }
}

impl<R: RemoteStorage + Clone> Gui<R> {
    pub fn new(rc: R, config: RfsConfig) -> Result<Self, GUIError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| GUIError::RunningIssue(e.to_string()))?;
        let ui = App::new().map_err(|e| GUIError::RenderingIssue(e.to_string()))?;

        Ok(Gui {
            ui,
            rc,
            rt: Arc::new(rt),
            daemon: Arc::new(Mutex::new(None)),
            default_config: config.clone(),
            config: Arc::new(Mutex::new(config.clone())),
        })
    }

    pub fn start_gui(self) -> Result<(), GUIError> {
        let ui_weak = Arc::new(self.ui.as_weak());

        /* OPENING LOGIC */
        let ui_health = ui_weak.clone();
        let rc_health = self.rc.clone();
        let rt_health = self.rt.clone();

        if let Some(ui) = ui_health.upgrade() {
            slint::Timer::single_shot(std::time::Duration::from_millis(2000), move || {
                tracing::info!("Server is available!");
                ui.set_active_view(ViewState::Home);
            });
        }

        self.ui.on_retry(move || {
            let rc_thread = rc_health.clone();
            let ui_thread = ui_health.clone();
            let rt_thread = rt_health.clone();

            rt_health.spawn(async move {
                let _ = health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;
            });
        });

        /* AUTHENTICATION MANAGEMENT */
        let ui_login = ui_weak.clone();
        let rc_login = self.rc.clone();
        let rt_login = self.rt.clone();
        let config_login = self.config.clone();
        self.ui.on_login(move |user, pass| {
            let username = user.to_string();
            let password = pass.to_string();

            let rc_thread = rc_login.clone();
            let ui_thread = ui_login.clone();
            let rt_thread = rt_login.clone();
            let config = config_login.clone();
            rt_login.spawn(async move {
                // 1. check connectivity with server
                let is_server_ok =
                    health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;

                if is_server_ok {
                    // 2. asyncronous call
                    let result = rc_thread.login(username, password).await;

                    // 3. result management and ui upgrade
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_thread.upgrade() {
                            match result {
                                Ok(_) => {
                                    // Load configuration
                                    if let Err(e) = load_config(&ui, config) {
                                        ui.set_error_message(e.to_string().into());
                                    }

                                    ui.set_is_loading(false);

                                    // Timer used to avoid RefCell panic
                                    slint::Timer::single_shot(
                                        std::time::Duration::from_millis(300),
                                        move || {
                                            if let Some(ui_final) = ui_thread.upgrade() {
                                                ui_final.set_status(Status::LoggedIn);
                                            }
                                        },
                                    );
                                }
                                Err(_) => {
                                    ui.set_error_message("Invalid credentials!".into());
                                }
                            }
                        }
                    });
                }
            });
        });

        let ui_logout = ui_weak.clone();
        let rc_logout = self.rc.clone();
        let rt_logout = self.rt.clone();
        let daemon_logout = self.daemon.clone();
        self.ui.on_logout(move || {
            let rc_thread = rc_logout.clone();
            let ui_thread = ui_logout.clone();
            let rt_thread = rt_logout.clone();
            let daemon_thread = daemon_logout.clone();
            rt_logout.spawn(async move {
                // 1. check connectivity with server
                let is_server_ok =
                    health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;

                if is_server_ok {
                    // 2. asyncronous call
                    let result = rc_thread.logout().await;

                    if result.is_ok() {
                        // 3. asynchronous unmount
                        let mut lock = daemon_thread.lock().await;
                        if let Some(daemon) = lock.take() {
                            daemon.trigger_shutdown();
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }

                    // 3. result management and ui upgrade
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_thread.upgrade() {
                            match result {
                                Ok(_) => {
                                    ui.set_fs_is_active(false);

                                    // Timer used to avoid RefCell panic
                                    slint::Timer::single_shot(
                                        std::time::Duration::from_millis(100),
                                        move || {
                                            if let Some(ui_final) = ui_thread.upgrade() {
                                                ui_final.set_status(Status::NotLoggedIn);
                                            }
                                        },
                                    );

                                    ui.set_is_loading(false);
                                }
                                Err(_) => {
                                    ui.set_error_message("Error while logging out!".into());
                                }
                            }
                        }
                    });
                }
            });
        });

        /* MOUNTING AND UNMOUNTING -> TODO: add support for Windows */
        let rc_mount = self.rc.clone();
        let rt_mount = self.rt.clone();
        let ui_mount = ui_weak.clone();
        let config_mount = self.config.clone();
        let daemon_mount = self.daemon.clone();
        self.ui.on_mount(move || {
            let ui_thread = ui_mount.clone();
            let config_arc = config_mount.clone();
            let rc_thread = rc_mount.clone();

            // 1. Extract config informations
            let config = config_arc.blocking_lock().clone();

            let daemon = Daemon::new();

            {
                let mut lock = daemon_mount.blocking_lock();
                *lock = Some(daemon.clone());
            }

            let _mount_options = MountOptions::from(&config.mount);

            tracing::info!("Starting FUSE runtime...");
            let ui_inner = ui_thread.clone();
            rt_mount.spawn(async move {
                let ui_gui = ui_inner.clone();
                if let Err(e) = run_async(config, rc_thread, daemon.clone()).await {
                    tracing::error!("Runtime crashed: {}", e);

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_gui.upgrade() {
                            ui.set_is_loading(false);
                            ui.set_fs_is_active(false);
                            ui.set_error_message(SharedString::from(format!(
                                "Runtime crushed: {e}"
                            )));
                        }
                    });
                };
            });

            let ui_gui = ui_thread.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_gui.upgrade() {
                    ui.set_is_loading(false);
                    ui.set_fs_is_active(true);
                }
            });
        });

        let ui_unmount = ui_weak.clone();
        let daemon_unmount = self.daemon.clone();
        self.ui.on_unmount(move || {
            let ui_thread = ui_unmount.clone();

            unmount(daemon_unmount.clone());

            let ui_inner = ui_thread.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_inner.upgrade() {
                    ui.set_fs_is_active(false);
                    ui.set_is_loading(false);
                }
            });
        });

        /* CONFIGURATION MANAGEMENT -> add input validation */
        let config_string = self.config.clone();
        let ui_string = ui_weak.clone();
        self.ui
            .on_change_string_property(move |object: StringItem| {
                match object.env.as_str() {
                    "MOUNT" => match object.id_rust.as_str() {
                        "mountpoint" => {
                            let mut config = config_string.blocking_lock();
                            config.mount_point = PathBuf::from(object.value.as_str());
                        }
                        "config_file" => {
                            let _config = config_string.blocking_lock();
                        }
                        _ => (),
                    },
                    "CACHE" => match object.id_rust.as_str() {
                        "policy" => {
                            let mut config = config_string.blocking_lock();
                            match object.value.as_str() {
                                "LRU" => {
                                    config.cache.policy = crate::config::CachePolicy::Lru;
                                }
                                "LFU" => {
                                    config.cache.policy = crate::config::CachePolicy::Lfu;
                                }
                                _ => (),
                            }
                        }
                        _ => (),
                    },
                    "LOG" => match object.id_rust.as_str() {
                        "log_format" => {
                            let mut config = config_string.blocking_lock();
                            match object.value.as_str() {
                                "COMPACT" => {
                                    config.logging.log_format = crate::config::LogFormat::Compact;
                                }
                                "FULL" => {
                                    config.logging.log_format = crate::config::LogFormat::Full;
                                }
                                "JSON" => {
                                    config.logging.log_format = crate::config::LogFormat::Json;
                                }
                                "PRETTY" => {
                                    config.logging.log_format = crate::config::LogFormat::Pretty;
                                }
                                _ => (),
                            }
                        }
                        "log_level" => {
                            let mut config = config_string.blocking_lock();
                            match object.value.as_str() {
                                "DEBUG" => {
                                    config.logging.log_level = crate::config::LogLevel::Debug;
                                }
                                "ERROR" => {
                                    config.logging.log_level = crate::config::LogLevel::Error;
                                }
                                "INFO" => {
                                    config.logging.log_level = crate::config::LogLevel::Info;
                                }
                                "TRACE" => {
                                    config.logging.log_level = crate::config::LogLevel::Trace;
                                }
                                "WARN" => {
                                    config.logging.log_level = crate::config::LogLevel::Warn;
                                }
                                _ => (),
                            }
                        }
                        "log_dir" => {
                            let mut config = config_string.blocking_lock();
                            config.logging.log_dir = Some(PathBuf::from(object.value.as_str()));
                        }
                        "log_file" => {
                            let mut config = config_string.blocking_lock();
                            config.logging.log_file = Some(PathBuf::from(object.value.as_str()));
                        }
                        "log_rotation" => {
                            let mut config = config_string.blocking_lock();
                            match object.value.as_str() {
                                "MINUTELY" => {
                                    config.logging.log_rotation =
                                        Some(crate::config::LogRotation::Minutely);
                                }
                                "HOURLY" => {
                                    config.logging.log_rotation =
                                        Some(crate::config::LogRotation::Hourly);
                                }
                                "DAILY" => {
                                    config.logging.log_rotation =
                                        Some(crate::config::LogRotation::Daily);
                                }
                                "NEVER" => {
                                    config.logging.log_rotation =
                                        Some(crate::config::LogRotation::Never);
                                }
                                _ => (),
                            }
                        }
                        _ => (),
                    },
                    _ => (),
                }

                // Update GUI values
                let ui_inner = ui_string.clone();
                let config_inner = config_string.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_inner.upgrade() {
                        ui.set_show_restore_default(true);
                        ui.set_is_loading(false);
                        if let Err(e) = load_config(&ui, config_inner) {
                            ui.set_error_message(e.to_string().into());
                        }
                    }
                });
            });

        let config_bool = self.config.clone();
        let ui_bool = ui_weak.clone();
        self.ui.on_change_bool_property(move |object: BoolItem| {
            match object.env.as_str() {
                "MOUNT" => match object.id_rust.as_str() {
                    "allow_other" => {
                        let mut config = config_bool.blocking_lock();
                        config.mount.allow_other = object.value;
                    }
                    "read_only" => {
                        let mut config = config_bool.blocking_lock();
                        config.mount.read_only = object.value;
                    }
                    "privileged" => {
                        let mut config = config_bool.blocking_lock();
                        config.mount.privileged = object.value;
                    }
                    _ => (),
                },
                "CACHE" => match object.id_rust.as_str() {
                    "cache_enabled" => {
                        let mut config = config_bool.blocking_lock();
                        config.cache.enabled = object.value;
                    }
                    "use_ttl" => {
                        let mut config = config_bool.blocking_lock();
                        config.cache.use_ttl = object.value;
                    }
                    _ => (),
                },
                "FS" => match object.id_rust.as_str() {
                    "xattr_enable" => {
                        let mut config = config_bool.blocking_lock();
                        config.file_system.xattr_enable = object.value;
                    }
                    _ => (),
                },
                _ => (),
            }

            // Update GUI values
            let ui_inner = ui_bool.clone();
            let config_inner = config_bool.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_inner.upgrade() {
                    ui.set_is_loading(false);
                    ui.set_show_restore_default(true);
                    if let Err(e) = load_config(&ui, config_inner) {
                        ui.set_error_message(e.to_string().into());
                    }
                }
            });
        });

        let config_int = self.config.clone();
        let ui_int = ui_weak.clone();
        self.ui.on_change_int_property( move |object: IntItem| {
            match object.env.as_str() {
                "CACHE" => {
                    match object.id_rust.as_str() {
                        "max_size" => {
                            let mut config = config_int.blocking_lock();
                            let capacity = config.cache.capacity;
                            if capacity * object.value as usize <= 1024*1024*1024 {
                                config.cache.max_size = object.value as usize;
                            }else {
                                let ui_inner = ui_int.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_inner.upgrade() {
                                        ui.set_is_loading(false);
                                        ui.set_error_message(format!("Considering the actual cache capacity and 1 GB cache limit, your maximum entry size can be {:.2} MB.", (1024 as f64) / (capacity as f64)).into())
                                    }
                                });
                            }
                        },
                        "capacity" => {
                            let mut config = config_int.blocking_lock();
                            let max_size = config.cache.max_size;
                            if max_size * object.value as usize <= 1024*1024*1024 {
                                config.cache.capacity = object.value as usize;
                            }else {
                                let ui_inner = ui_int.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_inner.upgrade() {
                                        ui.set_is_loading(false);
                                        ui.set_error_message(format!("Considering the actual cache entry max size and 1 GB cache limit, your maximum capacity can be {}.", 1024*1024*1024/max_size).into())
                                    }
                                });
                            }
                        },
                        "ttl" => {
                            let mut config = config_int.blocking_lock();
                            config.cache.ttl = object.value as u64;
                        },
                        _ => (),
                    }
                },
                "FS" => {
                    match object.id_rust.as_str() {
                        "buffer_size" => {
                            if object.value < 5 {
                                let mut config = config_int.blocking_lock();
                                config.file_system.buffer_size = object.value as usize;
                            }else{
                                let ui_inner = ui_int.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_inner.upgrade() {
                                        ui.set_is_loading(false);
                                        ui.set_error_message("Buffer size must be smaller than 5 MB.".into())
                                    }
                                });
                            }
                        },
                        _ => (),
                    }
                },
                _ => (),
            }

            // Update GUI values
            let ui_inner = ui_int.clone();
            let config_inner = config_int.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_inner.upgrade() {
                    ui.set_is_loading(false);
                    ui.set_show_restore_default(true);
                    if let Err(e) = load_config(&ui, config_inner){
                        ui.set_error_message(e.to_string().into());
                    }
                }
            });
        });

        let ui_restore = ui_weak.clone();
        let default_config = self.default_config.clone();
        self.ui.on_restore_default(move || {
            let ui_inner = ui_restore.clone();
            let config_inner = default_config.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_inner.upgrade() {
                    ui.set_is_loading(false);
                    ui.set_show_restore_default(false);
                    if let Err(e) = load_config(&ui, Arc::new(Mutex::new(config_inner))) {
                        ui.set_error_message(e.to_string().into());
                    }
                }
            });
        });

        self.ui
            .run()
            .map_err(|e| GUIError::RunningIssue(e.to_string()))?;

        Ok(())
    }
}

impl<R: RemoteStorage + Clone> Drop for Gui<R> {
    fn drop(&mut self) {
        let daemon_clone = self.daemon.clone();
        unmount(daemon_clone);

        if self.ui.get_status() == Status::LoggedIn {
            let rc_clone = self.rc.clone();
            self.rt.spawn(async move {
                let _ = rc_clone.logout().await;
            });
        }
    }
}

/* Creare funzione per gestire errori:
fn handle(fun: F -> Result<>) {
  match fun() {
    }
}
*/
