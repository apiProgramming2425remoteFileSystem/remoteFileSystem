slint::slint! {
    import { App, ViewState } from "src/gui/app.slint";
    export { App, ViewState }
}

use std::{path::PathBuf, sync::{Arc, Mutex}, fmt::Debug};

use slint::{SharedString, Weak};
use tokio::runtime::Runtime;
use crate::{config::RfsConfig, daemon::Daemon, error::GUIError, logging, network::RemoteStorage, run_async};


async fn health_check<R: RemoteStorage + Debug + Clone + 'static>(rc: R, ui_weak: Arc<Weak<App>>, rt: Arc<Runtime>) -> bool {

    let rc_thread = rc.clone();
    let ui_thread = ui_weak.clone();

    let handle = rt.spawn(async move {
        // 1. asyncronous call
        let result = rc_thread.health_check().await;

        let is_ok = match result {
            Ok(_) => true,
            Err(_) => false,
        };

        // 2. result management and ui upgrade
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_thread.upgrade() {
                ui.set_is_loading(false);

                match result {
                    Ok(_) => {
                        tracing::info!("Server is available!");
                        ui.set_server_unavailable(false);
                    },
                    Err(err) => {
                        tracing::error!("Server error: {err}");
                        ui.set_server_unavailable(true);
                    }
                }
            }
        });

        return is_ok;
    });

    handle.await.unwrap()
}

fn load_config(ui: &App, config: Arc<Mutex<RfsConfig>>) {
    let cfg = config.lock().unwrap();

    let log_dir: String = if let Some(dir) = &cfg.logging.log_dir{
        String::from(dir.to_string_lossy().into_owned())
    }else{
        String::from("")
    };

    let log_file: String = if let Some(file) = &cfg.logging.log_file {
        String::from(file.to_string_lossy().into_owned())
    }else{
        String::from("")
    };

    let log_rotation: String = if let Some(rotation) = &cfg.logging.log_rotation {
        rotation.to_string()
    }else{
        String::from("")
    };
    
    /* CONFIG MANAGEMENT */
    ui.set_mount_settings(MountSettings { allow_other: cfg.mount.allow_other, allow_root: cfg.mount.allow_root, mount_point: SharedString::from(cfg.mount_point.to_string_lossy().into_owned()), privileged: cfg.mount.privileged, read_only: cfg.mount.read_only });

    /* CACHE MANAGEMENT */
    ui.set_cache_settings(CacheSettings { capacity: cfg.cache.capacity as i32, enabled: cfg.cache.enabled, max_size: cfg.cache.max_size as i32, policy: SharedString::from(cfg.cache.policy.to_string()), ttl: cfg.cache.ttl as i32, use_ttl: cfg.cache.use_ttl });

    /* FS AND LOGGING MANAGEMENT */
    ui.set_fs_settings(FsSettings { buffer_size: cfg.file_system.buffer_size as i32, foreground: cfg.foreground, max_pages: cfg.file_system.max_pages as i32, page_size: cfg.file_system.page_size as i32, server_url: SharedString::from(&cfg.server_url), xattr_enable: cfg.file_system.xattr_enable });
    ui.set_log_settings(LogSettings { dir: SharedString::from(log_dir), file: SharedString::from(log_file), format: SharedString::from(cfg.logging.log_format.to_string()), level: SharedString::from(cfg.logging.log_level.to_string_gui()), rotation: SharedString::from(log_rotation) });
}

pub fn start_gui<R: RemoteStorage + Debug + Clone + 'static>(rc: R, config: RfsConfig) -> Result<(), GUIError> {
    let ui = App::new().map_err(|e| GUIError::RenderingIssue(e.to_string()))?;
    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| GUIError::RunningIssue(e.to_string()))?;

    let ui_handle = Arc::new(ui.as_weak());
    let rc_handle = rc.clone();
    let rt_handle = Arc::new(rt);
    let default_config = config.clone();
    let config_handle = Arc::new(std::sync::Mutex::new(config));

    /* OPEINING LOGIC */ 
    let rc_health = rc_handle.clone();
    let ui_health = ui_handle.clone();
    let rt_health = rt_handle.clone();

    if let Some(ui) = ui_health.upgrade() {
        slint::Timer::single_shot(std::time::Duration::from_millis(2000), move || {
            tracing::info!("Server is available!");
            ui.set_active_view(ViewState::Home);
        });
    }


    ui.on_retry(move || {
        let rc_thread = rc_health.clone();
        let ui_thread = ui_health.clone();
        let rt_thread = rt_health.clone();

        rt_health.spawn( async move {
            let _ = health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;
        });
    });

    /* AUTHENTICATION MANAGEMENT */
    let ui_login = ui_handle.clone();
    let rc_login = rc_handle.clone();
    let rt_login = rt_handle.clone();
    let config_login = config_handle.clone();
    ui.on_login(move |user, pass| {
        let username = user.to_string();
        let password = pass.to_string();

        let rc_thread = rc_login.clone();
        let ui_thread = ui_login.clone();
        let rt_thread = rt_login.clone();
        let config = config_login.clone();
        rt_login.spawn(async move {
            // 1. check connectivity with server 
            let is_server_ok = health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;
            
            if is_server_ok {
                // 2. asyncronous call
                let result = rc_thread.login(username, password).await;

                // 3. result management and ui upgrade
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_thread.upgrade() {

                        match result {
                            Ok(_) => {
                                // Load configuration
                                load_config(&ui, config);

                                // Timer used to avoid RefCell panic
                                slint::Timer::single_shot(std::time::Duration::from_millis(300), move || {
                                    if let Some(ui_final) = ui_thread.upgrade() {
                                        ui_final.set_status(Status::LoggedIn);
                                    }
                                });

                                ui.set_is_loading(false);
                            },
                            Err(_) => {
                                ui.set_error_message("Invalid credentials!".into());
                            }
                        }

                    }

                });
            }
        });
        
    });

    let ui_logout = ui_handle.clone();
    let rc_logout = rc_handle.clone();
    let rt_logout = rt_handle.clone();
    ui.on_logout(move || {

        let rc_thread = rc_logout.clone();
        let ui_thread = ui_logout.clone();
        let rt_thread = rt_logout.clone();

        rt_logout.spawn(async move {
            // 1. check connectivity with server 
            let is_server_ok = health_check(rc_thread.clone(), ui_thread.clone(), rt_thread.clone()).await;

            if is_server_ok {
                // 2. asyncronous call
                let result = rc_thread.logout().await;

                // 3. result management and ui upgrade
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_thread.upgrade() {

                        match result {
                            Ok(_) => {
                                ui.set_fs_is_active(false);

                                // Timer used to avoid RefCell panic
                                slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
                                    if let Some(ui_final) = ui_thread.upgrade() {
                                        ui_final.set_status(Status::NotLoggedIn);
                                    }
                                });

                                ui.set_is_loading(false);
                            },
                            Err(_) => {
                                ui.set_error_message("Error while logging out!".into());
                            }
                        }

                    }

                });
            }

        });

    });

    /* MOUNTING AND UNMOUNTING */
    let rc_mount = rc_handle.clone();
    let rt_mount = rt_handle.clone();
    let ui_mount = ui_handle.clone();
    let config_mount = config_handle.clone();
    ui.on_mount(move || {
        let ui_thread = ui_mount.clone();
        let config_arc = config_mount.clone();
        let rc_thread = rc_mount.clone();

        // Estraiamo i dati dal lock e lo rilasciamo subito
        let (config, daemon) = {
            let cfg = config_arc.lock().unwrap();
            (cfg.clone(), Daemon::new().foreground(cfg.foreground))
        };

        // 2. Inizializzazione (operazione veloce)
        if let Err(e) = daemon.initialize() {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_thread.upgrade() {
                    ui.set_is_loading(false);
                    ui.set_error_message(format!("Errore: {}", e).into());
                }
            });
            return;
        }

        // 4. PUNTO DI BLOCCO
        // Questa chiamata "sequestra" il thread fino all'unmount
        tracing::info!("Starting FUSE runtime (blocking thread)...");
        if let Err(e) = daemon.create_runtime(run_async(config, rc_thread, daemon.clone())) {
            tracing::error!("Runtime crashed: {}", e);
        };
        
        tracing::info!("FUSE runtime terminated gracefully.");

        let ui_inner = ui_thread.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_inner.upgrade() {
                ui.set_is_loading(false);
                ui.set_fs_is_active(true);
            }
        });
    });


    /* CONFIGURATION MANAGEMENT -> TO DO */
    let ui_mountpoint = ui_handle.clone();
    let config_mountpoint = config_handle.clone();
    /* 
    ui.on_select_mountpoint(move || {
        let ui_thread = ui_mountpoint.clone();

        let folder = FileDialog::new()
            .set_title("Select your mounting point")
            .pick_folder();

        if let Some(path) = folder {
            let path_str = path.display().to_string();
            
            // 2. Torniamo nel thread principale di Slint per aggiornare la UI
            let config = config_mountpoint.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_thread.upgrade() {
                    let mut cfg = config.lock().unwrap();
                    cfg.mount_point = PathBuf::from(path_str);
                    ui.set_mount_settings(MountSettings { allow_other: cfg.mount.allow_other, allow_root: cfg.mount.allow_root, mount_point: SharedString::from(cfg.mount_point.to_string_lossy().into_owned()), privileged: cfg.mount.privileged, read_only: cfg.mount.read_only });
                }
            });
        };
    });
    */

    /* 
    ui.on_change_string_setting( move |object: StringItem| {

    });
    */

    //ui.on_change_bool_setting()

    //ui.on_change_int_setting()

    
    ui.run().map_err(|e| GUIError::RunningIssue(e.to_string()))?;

    Ok(())
}

/* Creare funzione per gestire errori: 
handle(fun) {
  match fun() {
    }
}
*/