use std::path::PathBuf;
use std::sync::Mutex;

use dayz_core::launch::{build_launch_args, launch};
use dayz_core::mods::{download_mod, ensure_mod_symlinks, missing_mods, scan_installed_mods};
use dayz_core::process::RealRunner;
use dayz_core::profile::Profile;
use dayz_core::servers::{
    apply_filter, cache_read, cache_write, fetch_servers, fuzzy_search, Server, ServerFilter,
};
use dayz_core::steam::SteamInstall;
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub servers: Mutex<Vec<Server>>,
}

fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[tauri::command]
pub async fn list_servers(
    refresh: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Server>, String> {
    let cache = data_dir(&app).join("servers.json");
    let servers = if !refresh {
        match cache_read(&cache, 300) {
            Some(s) => s,
            None => fetch_and_cache(&cache).await?,
        }
    } else {
        fetch_and_cache(&cache).await?
    };
    *state.servers.lock().unwrap() = servers.clone();
    Ok(servers)
}

async fn fetch_and_cache(cache: &std::path::Path) -> Result<Vec<Server>, String> {
    let client = reqwest::Client::new();
    let servers = fetch_servers(&client).await.map_err(|e| e.to_string())?;
    let _ = cache_write(cache, &servers);
    Ok(servers)
}

#[tauri::command]
pub fn filter_servers(
    filter: ServerFilter,
    query: String,
    state: State<'_, AppState>,
) -> Vec<Server> {
    let servers = state.servers.lock().unwrap().clone();
    let filtered = apply_filter(&servers, &filter);
    fuzzy_search(&filtered, &query)
}

#[tauri::command]
pub async fn play(
    server: Server,
    password: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let runner = RealRunner::new();
    let steam = SteamInstall::detect_in(&home()).map_err(|e| e.to_string())?;
    let workshop = steam.workshop_dir();
    let game = steam.game_dir();

    let profile = Profile::load(&data_dir(&app).join("profile.json"));
    let installed = scan_installed_mods(&workshop);
    let missing = missing_mods(&server.mods, &installed);

    for id in &missing {
        app.emit("mod-progress", format!("Installing mod {id}"))
            .ok();
        download_mod(&runner, &profile.steam_login, *id)
            .await
            .map_err(|e| e.to_string())?;
    }

    let ids: Vec<u64> = server.mods.iter().map(|m| m.workshop_id).collect();
    ensure_mod_symlinks(&game, &workshop, &ids).map_err(|e| e.to_string())?;

    let args = build_launch_args(&server, &profile.player, password.as_deref());
    launch(&runner, &args).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_profile(app: tauri::AppHandle) -> Profile {
    Profile::load(&data_dir(&app).join("profile.json"))
}

#[tauri::command]
pub fn save_profile(profile: Profile, app: tauri::AppHandle) -> Result<(), String> {
    profile
        .save(&data_dir(&app).join("profile.json"))
        .map_err(|e| e.to_string())
}
