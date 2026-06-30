use std::path::PathBuf;
use std::sync::Mutex;

use dayz_core::launch::{build_launch_args, launch};
use dayz_core::mods::{
    dir_size_bytes, download_mod, ensure_mod_symlinks, lowercase_mod_tree, missing_mods,
    scan_installed_mods,
};
use dayz_core::process::{
    detect_terminal, program_available, spawn_detached, terminal_login_command, RealRunner,
    DEFAULT_TERMINALS,
};
use dayz_core::profile::{Profile, ServerRef};
use dayz_core::servers::{
    apply_filter, cache_read, cache_write, dedupe_by_endpoint, fetch_mod_sizes, fetch_servers,
    fuzzy_search, Server, ServerFilter,
};
use dayz_core::steam::{app_launch_ready, dayz_app_state, locate_dayz, SteamInstall};
use dayz_core::version::{read_installed_version, version_match};
use tauri::{Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub servers: Mutex<Vec<Server>>,
    /// `Some` while a `play` is running; cancelling the token aborts the launch.
    pub launch: Mutex<Option<CancellationToken>>,
    /// Memoized installed DayZ build (outer `None` = not computed yet; inner `None` = couldn't
    /// detect). Reading it scans the ~18 MB game exe, so we do it once per session.
    pub dayz_version: Mutex<Option<Option<String>>>,
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
        E::DayzNotFound => cmd_err(
            "dayz_not_found",
            "DayZ was not found in any Steam library. Set the DayZ folder in Settings, \
             or restart Steam with the drive mounted so it re-registers the library.",
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
    if !refresh {
        // Stale-while-revalidate: return instantly from whatever we already have so the UI
        // can render immediately; the frontend kicks off a background `refresh=true` to update.
        {
            let in_mem = state.servers.lock().unwrap();
            if !in_mem.is_empty() {
                return Ok(in_mem.clone());
            }
        }
        // No in-memory list yet — serve the disk cache even if stale (ignore TTL); only hit
        // the network when there is no cache file at all (truly first run).
        if let Some(servers) = cache_read(&cache, u64::MAX) {
            // Sanitize stale caches written before endpoint dedup, so a duped cache can't crash
            // the UI (the network path is already deduped in `parse_servers`).
            let servers = dedupe_by_endpoint(servers);
            *state.servers.lock().unwrap() = servers.clone();
            return Ok(servers);
        }
    }
    let servers = fetch_and_cache(&cache).await?;
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

/// Installed DayZ build, memoized in `AppState`. Locates DayZ (honoring the user's override) and
/// reads its exe once per session; later calls reuse the cached value.
fn installed_dayz_version(app: &tauri::AppHandle, state: &AppState) -> Option<String> {
    if let Some(cached) = state.dayz_version.lock().unwrap().as_ref() {
        return cached.clone();
    }
    let profile = Profile::load(&data_dir(app).join("profile.json"));
    let computed = locate_dayz(&home(), profile.steam_root.as_deref())
        .ok()
        .and_then(|d| read_installed_version(&d.game_dir()));
    *state.dayz_version.lock().unwrap() = Some(computed.clone());
    computed
}

#[tauri::command]
pub fn filter_servers(
    filter: ServerFilter,
    query: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Vec<Server> {
    let servers = state.servers.lock().unwrap().clone();
    let filtered = apply_filter(&servers, &filter);
    let mut result = fuzzy_search(&filtered, &query);

    // Annotate each row against the installed build so the UI can flag mismatches, then optionally
    // hide them. `version_match` is `None` when either version is unknown — never hidden.
    let local = installed_dayz_version(&app, &state);
    for s in &mut result {
        s.version_match = version_match(&s.version, local.as_deref());
    }
    if filter.same_version_only {
        result.retain(|s| s.version_match != Some(false));
    }
    result
}

/// Structured launch-progress payload emitted on the `launch-progress` event so the
/// frontend can drive its blocking dialog (spinner + step-based mod download bar).
#[derive(Clone, serde::Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum LaunchProgress {
    Preparing,
    Downloading {
        current: usize,
        total: usize,
        id: u64,
        name: String,
        /// Bytes written to SteamCMD's staging dir so far (live, best-effort).
        bytes: u64,
        /// Total download size in bytes from Steam's workshop metadata, when known.
        /// `None` when the size lookup failed; the frontend then shows bytes alone.
        total_bytes: Option<u64>,
    },
    Launching,
}

#[tauri::command]
pub async fn play(
    server: Server,
    password: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    // Single-flight: refuse to start a second launch while one is already running.
    let token = {
        let mut guard = state.launch.lock().unwrap();
        if guard.is_some() {
            return Err(cmd_err("busy", "A launch is already in progress.", None));
        }
        let token = CancellationToken::new();
        *guard = Some(token.clone());
        token
    };

    let result = run_play(&server, password, &app, &token).await;

    // Always clear the in-flight token, whatever the outcome.
    *state.launch.lock().unwrap() = None;
    result
}

/// Cancel an in-progress launch: aborts the current SteamCMD download (the dropped future is
/// killed via `kill_on_drop`) and makes `play` return a `cancelled` error.
#[tauri::command]
pub fn cancel_play(state: State<'_, AppState>) {
    if let Some(token) = state.launch.lock().unwrap().as_ref() {
        token.cancel();
    }
}

async fn run_play(
    server: &Server,
    password: Option<String>,
    app: &tauri::AppHandle,
    token: &CancellationToken,
) -> Result<(), CommandError> {
    app.emit("launch-progress", LaunchProgress::Preparing).ok();

    let runner = RealRunner::new();
    let mut profile = Profile::load(&data_dir(app).join("profile.json"));

    // Without a player name DayZ launches into the default "Survivor" profile. Fail fast with
    // actionable guidance rather than launching with a name the user didn't intend.
    if profile.player.trim().is_empty() {
        return Err(cmd_err(
            "no_player_name",
            "Set a player name in Settings (and click Save) before playing — without one DayZ \
             uses the default \"Survivor\" character.",
            None,
        ));
    }

    let dayz =
        locate_dayz(&home(), profile.steam_root.as_deref()).map_err(|e| to_command_error(&e))?;

    // Preflight: Steam refuses to launch an app that isn't cleanly installed (e.g. StateFlags=6,
    // "update required"), failing with a cryptic "Invalid platform". `steam -applaunch` hands off
    // and exits 0, so we can't detect that after the fact — check the appmanifest up front and bail
    // with actionable guidance before spending time downloading mods. A missing/unparseable
    // manifest yields `None`, in which case we proceed rather than block.
    if let Some(flags) = dayz_app_state(&dayz.root) {
        if !app_launch_ready(flags) {
            return Err(cmd_err(
                "dayz_not_ready",
                "DayZ isn't ready to launch in Steam — it has a pending update or repair. Open \
                 Steam and let DayZ finish updating, or right-click DayZ → Properties → Installed \
                 Files → Verify integrity. Also make sure DayZ has a Proton compatibility tool \
                 (Properties → Compatibility → force Proton), then try again.",
                Some(format!("appmanifest StateFlags = {flags}")),
            ));
        }
    }

    let workshop = dayz.workshop_dir();
    let game = dayz.game_dir();

    // Private HOME for every SteamCMD run so its bootstrap can't rewrite the Steam client's shared
    // `libraryfolders.vdf` and drop the DayZ library (see `dayz_core::process::with_home`).
    let steamcmd_home = data_dir(app).join("steamcmd-home");
    std::fs::create_dir_all(&steamcmd_home).ok();

    let installed = scan_installed_mods(&workshop);
    let missing = missing_mods(&server.mods, &installed);

    // Best-effort total download sizes so the progress dialog can show "X MB of Y MB".
    // An empty map (network/parse failure) just falls back to a totals-less display.
    let sizes = fetch_mod_sizes(&reqwest::Client::new(), &missing).await;

    let total = missing.len();
    for (i, id) in missing.iter().enumerate() {
        let name = server
            .mods
            .iter()
            .find(|m| m.workshop_id == *id)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| id.to_string());
        app.emit(
            "launch-progress",
            LaunchProgress::Downloading {
                current: i + 1,
                total,
                id: *id,
                name: name.clone(),
                bytes: 0,
                total_bytes: sizes.get(id).copied(),
            },
        )
        .ok();
        // Race the download against cancellation while polling its on-disk staging dir for live
        // byte progress. On cancel the download future is dropped, which kills the SteamCMD child
        // (RealRunner sets `kill_on_drop`).
        let downloads = dayz.workshop_downloads_dir().join(id.to_string());
        let download = download_mod(&runner, &steamcmd_home, &profile.steam_login, &dayz.root, *id);
        tokio::pin!(download);
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        tick.tick().await; // first tick is immediate; skip it (we just emitted bytes: 0)
        let res = loop {
            tokio::select! {
                r = &mut download => break r,
                _ = token.cancelled() => {
                    return Err(cmd_err("cancelled", "Launch cancelled.", None));
                }
                _ = tick.tick() => {
                    app.emit(
                        "launch-progress",
                        LaunchProgress::Downloading {
                            current: i + 1,
                            total,
                            id: *id,
                            name: name.clone(),
                            bytes: dir_size_bytes(&downloads),
                            total_bytes: sizes.get(id).copied(),
                        },
                    )
                    .ok();
                }
            }
        };
        res.map_err(|e| to_command_error(&e))?;
        // Verify the mod actually landed on disk — SteamCMD can exit 0 without writing files,
        // so don't trust the exit code alone. `download_mod` passes `+force_install_dir` so
        // content lands in this same `workshop` tree; if it's still missing the most likely
        // cause is an incomplete SteamCMD login (cached credentials expired).
        if !workshop.join(id.to_string()).join("meta.cpp").exists() {
            return Err(cmd_err(
                "mod_missing",
                format!(
                    "Mod {id} downloaded but its files weren't found. Open Settings, confirm \
                     SteamCMD is installed and you've completed \"Set up Steam login\", then \
                     try again."
                ),
                None,
            ));
        }
    }

    app.emit("launch-progress", LaunchProgress::Launching).ok();

    let ids: Vec<u64> = server.mods.iter().map(|m| m.workshop_id).collect();
    // DayZ runs under Proton on a case-sensitive filesystem and only finds lowercase `addons\`.
    // Many mods ship a capital `Addons/`, so normalize each mod's tree before linking it in,
    // otherwise the server reports "Missing PBO". Idempotent, so safe to repeat every launch.
    for id in &ids {
        lowercase_mod_tree(&workshop.join(id.to_string())).map_err(|e| to_command_error(&e))?;
    }
    ensure_mod_symlinks(&game, &workshop, &ids).map_err(|e| to_command_error(&e))?;

    let args = build_launch_args(server, &profile.player, password.as_deref());
    launch(&args).map_err(|e| to_command_error(&e))?;

    // Record the launch in history (most recent first, deduped by ip+port). Best-effort: a
    // failed save must not fail an already-launched game.
    profile.add_history(
        ServerRef {
            name: server.name.clone(),
            ip: server.ip.clone(),
            port: server.game_port,
        },
        50,
    );
    let _ = profile.save(&data_dir(app).join("profile.json"));
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

/// Add or remove a server from favorites (by ip+port) and return the updated profile.
#[tauri::command]
pub fn toggle_favorite(server_ref: ServerRef, app: tauri::AppHandle) -> Profile {
    let path = data_dir(&app).join("profile.json");
    let mut profile = Profile::load(&path);
    profile.toggle_favorite(server_ref);
    let _ = profile.save(&path);
    profile
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
    // Same isolated HOME the mod downloads use, so the cached credentials land where SteamCMD will
    // later look for them (see `dayz_core::process::with_home`).
    let steamcmd_home = data_dir(&app).join("steamcmd-home");
    std::fs::create_dir_all(&steamcmd_home).ok();
    let (prog, args) = terminal_login_command(&term, &steamcmd_home, &profile.steam_login);
    spawn_detached(&prog, &args).map_err(|e| to_command_error(&e))
}

/// Diagnostics for the Settings tab: app version, dependency availability, and Steam install.
#[derive(Debug, serde::Serialize)]
pub struct EnvReport {
    pub app_version: String,
    pub steamcmd_installed: bool,
    pub terminal: Option<String>,
    pub steam_found: bool,
    pub steam_kind: Option<String>,
    pub steam_root: Option<String>,
    pub dayz_installed: bool,
    pub dayz_path: Option<String>,
    pub dayz_version: Option<String>,
    pub steam_login: String,
}

/// Probe the environment so the user can see whether everything needed to install mods and
/// launch is present. Infallible — missing pieces are reported as `false`/`None`.
#[tauri::command]
pub async fn check_environment(app: tauri::AppHandle) -> EnvReport {
    let runner = RealRunner::new();
    let steamcmd_installed = program_available(&runner, "steamcmd").await;
    let terminal = detect_terminal(DEFAULT_TERMINALS);
    let profile = Profile::load(&data_dir(&app).join("profile.json"));

    let (steam_found, steam_kind, steam_root) = match SteamInstall::detect_in(&home()) {
        Ok(s) => (
            true,
            Some(format!("{:?}", s.kind)),
            Some(s.root.display().to_string()),
        ),
        Err(_) => (false, None, None),
    };

    // DayZ may live in a different Steam library than the client root, so resolve it
    // independently (honoring the user's override) rather than assuming the client root.
    let (dayz_installed, dayz_path, dayz_version) =
        match locate_dayz(&home(), profile.steam_root.as_deref()) {
            Ok(d) => (
                true,
                Some(d.game_dir().display().to_string()),
                read_installed_version(&d.game_dir()),
            ),
            Err(_) => (false, None, None),
        };

    EnvReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        steamcmd_installed,
        terminal,
        steam_found,
        steam_kind,
        steam_root,
        dayz_installed,
        dayz_path,
        dayz_version,
        steam_login: profile.steam_login,
    }
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
