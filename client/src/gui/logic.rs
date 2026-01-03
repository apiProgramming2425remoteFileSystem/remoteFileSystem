slint::slint! {
    import { App, ViewState } from "src/gui/app.slint";
    export { App, ViewState }
}

use slint::SharedString;
use crate::{config::Config, error::{ClientError, GUIError}, network::RemoteClient};

pub fn start_gui(rc: RemoteClient, config: &Config) -> Result<(), GUIError> {
    let ui = App::new().map_err(|e| GUIError::RenderingIssue(e.to_string()))?;
    let ui_handle = ui.as_weak();

    /* Welcome page + server health check -> Home page */ 
    let rc_clone = rc.clone();
    let health_result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| GUIError::RenderingIssue(e.to_string()))?
        .block_on(rc_clone.health_check());

    if let Some(ui) = ui_handle.upgrade() {
        match health_result {
            Ok(_) => {
                slint::Timer::single_shot(std::time::Duration::from_millis(2000), move || {
                    tracing::info!("Server is available!");
                    ui.set_active_view(ViewState::Home);
                });
            },
            Err(err) => {
                tracing::error!("Server error: {err}");
                ui.set_error_message(SharedString::from(format!("Server error: {err}")));
            },
        }
    }

    /* Login management */
    let login_ui_handle = ui_handle.clone();
    ui.on_login_attempted(move |user, pass| {
        let username = user.to_string();
        let password = pass.to_string();
        let rc_clone = rc.clone();

        if let Some(ui) = login_ui_handle.upgrade() {

            let runtime_result = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build();

            match runtime_result {
                Ok(runtime) => {
                    let _: Result<(), ClientError> = runtime.block_on(async {
                        let token_option = match rc_clone.login(username, password).await {
                            Ok(t) => {
                                ui.set_status(Status::LoggedIn);
                                Some(t)
                            },
                            Err(_) => {
                                ui.set_error_message(SharedString::from(format!("Invalid credentials!")));
                                None
                            }
                        };

                        Ok(())
                    });
                }
                Err(err) => {
                    tracing::error!("Client error: {err}");
                    ui.set_error_message(SharedString::from(format!("Client error: {err}")));
                }
            }
            ui.set_is_loading(false);
        }
    });

    
    
    ui.run().map_err(|e| GUIError::RunningIssue(e.to_string()))?;
    Ok(())
}