mod commands;

use std::sync::Mutex;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Log in release too, so steamcmd failures are diagnosable (writes to the
            // app log dir + stdout).
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            Ok(())
        })
        .manage(AppState {
            servers: Mutex::new(Vec::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::filter_servers,
            commands::play,
            commands::get_profile,
            commands::save_profile,
            commands::setup_steam_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
