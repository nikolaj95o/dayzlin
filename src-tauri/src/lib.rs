mod commands;

use std::sync::Mutex;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Log in release too, so mod-download and launch failures are diagnosable (writes
            // to the app log dir + stdout).
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            // Sweep up any mod downloads left over from a cancelled/failed launch. Safe only with
            // Steam closed, which `reconcile_downloads` checks; if Steam is up, the leftovers wait
            // until the user runs the Settings cleanup (or the next startup with Steam closed).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let report = commands::reconcile_downloads(&handle).await;
                if report.removed > 0 {
                    log::info!("cleaned up {} leftover mod download(s)", report.removed);
                }
            });
            Ok(())
        })
        .manage(AppState {
            servers: Mutex::new(Vec::new()),
            launch: Mutex::new(None),
            dayz_version: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::filter_servers,
            commands::play,
            commands::cancel_play,
            commands::get_profile,
            commands::save_profile,
            commands::toggle_favorite,
            commands::check_environment,
            commands::cleanup_downloads,
            commands::resolve_dayz_path,
            commands::list_installed_mods,
            commands::delete_installed_mod,
            commands::open_workshop_page,
            commands::open_mod_folder,
            commands::check_for_updates,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
