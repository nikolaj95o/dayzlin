use std::path::PathBuf;
use std::sync::Mutex;

use dayz_core::launch::{build_launch_args, launch};
use dayz_core::mods::{download_mod, ensure_mod_symlinks, missing_mods, scan_installed_mods};
use dayz_core::process::{
    detect_terminal, spawn_detached, terminal_login_command, RealRunner, DEFAULT_TERMINALS,
};
use dayz_core::profile::Profile;
use dayz_core::servers::{
    apply_filter, cache_read, cache_write, fetch_servers, fuzzy_search, Server, ServerFilter,
};
use dayz_core::steam::SteamInstall;
use tauri::{Emitter, Manager, State};

pub struct AppState {
    pub servers: Mutex<Vec<Server>>,
}

/// Structured error surfaced to the frontend, which renders it in a dialog.
#[derive(Debug, serde::Serialize)]
pub struct CommandError {
    pub kind: String,
    pub message: String,
    pub detail: Option<String>,
}

fn cmd_err(kind: &str, message: impl Into<String>, detail: Option<String>) -> CommandError {
    CommandError {
        kind: kind.into(),
        message: message.into(),
        detail,
    }
}

/// Map a `dayz_core::Error` to an actionable, user-facing message (per design spec).
fn to_command_error(e: &dayz_core::Error) -> CommandError {
    use dayz_core::Error as E;
    match e {
        E::SteamCmdLogin { detail } => cmd_err(
            "steam_login",
            "Steam needs a one-time login. Open Settings and click \"Set up Steam login\" \
             (or run `steamcmd +login <user> +quit` in a terminal and complete Steam Guard), \
             then try again. Close the Steam client first.",
            Some(detail.clone()),
        ),
        E::AnonymousAccount => cmd_err(
            "steam_login_missing",
            "Set your Steam account name in Settings before installing mods. \
             Anonymous Steam accounts cannot download DayZ mods.",
            None,
        ),
        E::SteamNotFound => cmd_err(
            "steam_not_found",
            "Steam with DayZ installed was not found. Install Steam and DayZ, then restart dayzlin.",
            None,
        ),
        E::ModNotInstalled(id) => cmd_err(
            "mod_missing",
            format!("Mod {id} is required but is not installed yet."),
            None,
        ),
        E::Network(msg) => cmd_err(
            "network",
            "Network error while contacting the server list.",
            Some(msg.clone()),
        ),
        E::CommandFailed {
            program,
            status,
            stderr,
        } => cmd_err(
            "command_failed",
            format!("`{program}` failed (status {status})."),
            Some(stderr.clone()),
        ),
        other => cmd_err("error", other.to_string(), None),
    }
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
) -> Result<Vec<Server>, CommandError> {
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

async fn fetch_and_cache(cache: &std::path::Path) -> Result<Vec<Server>, CommandError> {
    let client = reqwest::Client::new();
    let servers = fetch_servers(&client)
        .await
        .map_err(|e| to_command_error(&e))?;
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
) -> Result<(), CommandError> {
    let runner = RealRunner::new();
    let steam = SteamInstall::detect_in(&home()).map_err(|e| to_command_error(&e))?;
    let workshop = steam.workshop_dir();
    let game = steam.game_dir();

    let profile = Profile::load(&data_dir(&app).join("profile.json"));
    let installed = scan_installed_mods(&workshop);
    let missing = missing_mods(&server.mods, &installed);

    for id in &missing {
        app.emit("mod-progress", format!("Installing mod {id}…"))
            .ok();
        download_mod(&runner, &profile.steam_login, *id)
            .await
            .map_err(|e| to_command_error(&e))?;
    }

    let ids: Vec<u64> = server.mods.iter().map(|m| m.workshop_id).collect();
    ensure_mod_symlinks(&game, &workshop, &ids).map_err(|e| to_command_error(&e))?;

    let args = build_launch_args(&server, &profile.player, password.as_deref());
    launch(&runner, &args)
        .await
        .map_err(|e| to_command_error(&e))?;
    Ok(())
}

#[tauri::command]
pub fn get_profile(app: tauri::AppHandle) -> Profile {
    Profile::load(&data_dir(&app).join("profile.json"))
}

#[tauri::command]
pub fn save_profile(profile: Profile, app: tauri::AppHandle) -> Result<(), CommandError> {
    profile
        .save(&data_dir(&app).join("profile.json"))
        .map_err(|e| to_command_error(&e))
}

/// Open a terminal running an interactive `steamcmd +login <user>` for first-time auth.
#[tauri::command]
pub fn setup_steam_login(app: tauri::AppHandle) -> Result<(), CommandError> {
    let profile = Profile::load(&data_dir(&app).join("profile.json"));
    if profile.steam_login.is_empty() || profile.steam_login == "anonymous" {
        return Err(to_command_error(&dayz_core::Error::AnonymousAccount));
    }
    let term = detect_terminal(DEFAULT_TERMINALS).ok_or_else(|| {
        cmd_err(
            "no_terminal",
            format!(
                "No terminal emulator was found. Open one yourself and run: \
                 steamcmd +login {} +quit",
                profile.steam_login
            ),
            None,
        )
    })?;
    let (prog, args) = terminal_login_command(&term, &profile.steam_login);
    spawn_detached(&prog, &args).map_err(|e| to_command_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dayz_core::Error;

    #[test]
    fn maps_steam_login_error_with_detail() {
        let ce = to_command_error(&Error::SteamCmdLogin {
            detail: "FAILED login with result code Invalid Password".into(),
        });
        assert_eq!(ce.kind, "steam_login");
        assert!(ce.message.contains("Set up Steam login"));
        assert!(ce.detail.unwrap().contains("Invalid Password"));
    }

    #[test]
    fn maps_anonymous_and_steam_not_found() {
        assert_eq!(
            to_command_error(&Error::AnonymousAccount).kind,
            "steam_login_missing"
        );
        assert_eq!(
            to_command_error(&Error::SteamNotFound).kind,
            "steam_not_found"
        );
    }
}
