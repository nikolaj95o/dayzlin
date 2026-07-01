use std::path::PathBuf;
use std::sync::Mutex;

use dayz_core::launch::{build_launch_args, launch};
use dayz_core::mods::{
    dir_size_bytes, ensure_mod_symlinks, is_download_complete, lowercase_mod_tree, missing_mods,
    remove_workshop_download, scan_installed_mods, workshop_download_url,
};
use dayz_core::process::{spawn_detached, steam_running, RealRunner};
use dayz_core::profile::{Profile, ServerRef};
use dayz_core::servers::{
    apply_filter, cache_read, cache_write, dedupe_by_endpoint, fetch_mod_sizes, fetch_servers,
    fuzzy_search, Server, ServerFilter,
};
use dayz_core::steam::{app_launch_ready, dayz_app_state, library_root, locate_dayz, SteamInstall};
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

/// Workshop ids dayzlin asked Steam to download but hasn't yet seen finish. Persisted so a
/// download the user cancelled (or that failed) can be cleaned up later — Steam leaves such an
/// item downloading forever otherwise (see [`remove_workshop_download`]). Lives next to
/// `profile.json` in the app data dir.
fn pending_downloads_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("pending_downloads.json")
}

fn read_pending_downloads(path: &std::path::Path) -> Vec<u64> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<u64>>(&s).ok())
        .unwrap_or_default()
}

fn write_pending_downloads(path: &std::path::Path, ids: &[u64]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(ids) {
        let _ = std::fs::write(path, json);
    }
}

fn add_pending_download(path: &std::path::Path, id: u64) {
    let mut ids = read_pending_downloads(path);
    if !ids.contains(&id) {
        ids.push(id);
        write_pending_downloads(path, &ids);
    }
}

fn remove_pending_download(path: &std::path::Path, id: u64) {
    let mut ids = read_pending_downloads(path);
    if ids.contains(&id) {
        ids.retain(|x| *x != id);
        write_pending_downloads(path, &ids);
    }
}

/// Result of a cleanup pass over leftover (cancelled/failed) mod downloads.
#[derive(Debug, serde::Serialize)]
pub struct CleanupReport {
    /// Steam was running, so nothing could be safely touched (Steam rewrites its manifest while up).
    pub steam_running: bool,
    /// Leftover downloads actually removed this pass.
    pub removed: usize,
    /// Leftover downloads still waiting to be cleaned (non-zero only when `steam_running`).
    pub pending: usize,
}

/// Reconcile `pending_downloads.json` against the real workshop state. Ids that finished installing
/// are simply forgotten. For the rest — genuine leftovers — cleanup only happens with Steam closed,
/// since [`remove_workshop_download`] edits files Steam owns while running; otherwise we report the
/// count so the UI can ask the user to close Steam.
pub async fn reconcile_downloads(app: &tauri::AppHandle) -> CleanupReport {
    let path = pending_downloads_path(app);
    let ids = read_pending_downloads(&path);
    if ids.is_empty() {
        return CleanupReport {
            steam_running: false,
            removed: 0,
            pending: 0,
        };
    }

    let profile = Profile::load(&data_dir(app).join("profile.json"));
    let Ok(dayz) = locate_dayz(&home(), profile.steam_root.as_deref()) else {
        // Can't resolve DayZ right now; leave the list for a later attempt.
        return CleanupReport {
            steam_running: false,
            removed: 0,
            pending: ids.len(),
        };
    };

    // Drop ids that actually completed (they're installed now — nothing to clean).
    let workshop = dayz.workshop_dir();
    let mut leftovers: Vec<u64> = ids
        .into_iter()
        .filter(|id| !is_download_complete(&workshop, *id))
        .collect();

    if steam_running(&RealRunner::new()).await {
        // Persist just the still-incomplete leftovers; don't touch Steam's files while it's up.
        write_pending_downloads(&path, &leftovers);
        return CleanupReport {
            steam_running: true,
            removed: 0,
            pending: leftovers.len(),
        };
    }

    let mut removed = 0;
    leftovers.retain(|id| match remove_workshop_download(&dayz, *id) {
        Ok(()) => {
            removed += 1;
            false
        }
        Err(e) => {
            log::warn!("failed to clean up leftover download {id}: {e}");
            true
        }
    });
    write_pending_downloads(&path, &leftovers);
    CleanupReport {
        steam_running: false,
        removed,
        pending: leftovers.len(),
    }
}

/// Clean up leftover (cancelled/failed) mod downloads. Invoked from Settings and at startup.
#[tauri::command]
pub async fn cleanup_downloads(app: tauri::AppHandle) -> CleanupReport {
    reconcile_downloads(&app).await
}

/// How long a disk cache of the server list is considered fresh. Within this window a launch
/// serves the cache and skips the background network refresh entirely.
const CACHE_TTL_SECS: u64 = 300;

/// Load the server list into `AppState.servers`. Returns whether the served data is *stale* — i.e.
/// the frontend should kick off a background `refresh=true` to revalidate. The server data itself
/// flows to the UI via `filter_servers`; only the freshness signal is returned here.
#[tauri::command]
pub async fn list_servers(
    refresh: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    let cache = data_dir(&app).join("servers.json");
    if !refresh {
        // Already loaded this session — nothing to do, and no refresh needed.
        {
            let in_mem = state.servers.lock().unwrap();
            if !in_mem.is_empty() {
                return Ok(false);
            }
        }
        // Fresh disk cache (within TTL): serve it and skip the network entirely.
        if let Some(servers) = cache_read(&cache, CACHE_TTL_SECS) {
            *state.servers.lock().unwrap() = dedupe_by_endpoint(servers);
            return Ok(false);
        }
        // Stale disk cache: serve it instantly (stale-while-revalidate) but signal the frontend
        // to refresh in the background. Sanitize caches written before endpoint dedup so a duped
        // cache can't crash the UI (the network path is already deduped in `parse_servers`).
        if let Some(servers) = cache_read(&cache, u64::MAX) {
            *state.servers.lock().unwrap() = dedupe_by_endpoint(servers);
            return Ok(true);
        }
        // No cache at all (true first run): fetch now; the fresh result needs no refresh.
    }
    let servers = fetch_and_cache(&cache).await?;
    *state.servers.lock().unwrap() = servers;
    Ok(false)
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
        /// Bytes written to Steam's workshop staging dir so far (live, best-effort).
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

/// Cancel an in-progress launch: stops waiting on the current mod download and makes `play` return
/// a `cancelled` error. Steam may finish an already-started download in the background (there's no
/// child process we own to kill); it's simply reused on the next launch.
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

    let installed = scan_installed_mods(&workshop);
    let missing = missing_mods(&server.mods, &installed);

    // Mods download by asking the running Steam client to fetch them (see `workshop_download_url`),
    // so a live, logged-in client is required. Fail fast with actionable guidance rather than firing
    // `steam://` URLs that would silently do nothing (or cold-start Steam mid-launch).
    if !missing.is_empty() && !steam_running(&runner).await {
        return Err(cmd_err(
            "steam_not_running",
            "Open Steam and log in, then try again — dayzlin downloads mods through the running \
             Steam client.",
            None,
        ));
    }

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
        // Ask the running Steam client to fetch this item once, then watch the filesystem for it to
        // land — there's no child process to await (contrast the old SteamCMD path). Steam writes
        // in-progress data under `workshop/downloads/<id>` and only creates `content/<id>/meta.cpp`
        // when the item is complete.
        // Record the request before firing it: `workshop_download_item` is fire-and-forget and
        // Steam keeps re-downloading a cancelled item, so we track it for later cleanup and only
        // clear the entry once the item finishes landing.
        let pending_path = pending_downloads_path(app);
        add_pending_download(&pending_path, *id);
        let url = workshop_download_url(*id);
        spawn_detached("steam", &[url]).map_err(|e| to_command_error(&e))?;

        let downloads = dayz.workshop_downloads_dir().join(id.to_string());
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        // First tick is immediate; skip it (we just emitted bytes: 0).
        tick.tick().await;
        // Guardrails so a stuck download never hangs the launch: bail if nothing starts downloading
        // (Steam not ready / user dismissed the prompt) or if a started download stops making
        // progress. Cancelling via the UI still returns promptly through the `select!` below.
        let start = std::time::Instant::now();
        let mut last_bytes = 0u64;
        let mut last_progress = std::time::Instant::now();
        const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
        const STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    // No child to kill: Steam may finish the download in the background, which is
                    // fine — it'll be reused next launch. Cancel just stops us waiting/launching.
                    return Err(cmd_err("cancelled", "Launch cancelled.", None));
                }
                _ = tick.tick() => {
                    if is_download_complete(&workshop, *id) {
                        remove_pending_download(&pending_path, *id);
                        break;
                    }
                    let bytes = dir_size_bytes(&downloads);
                    app.emit(
                        "launch-progress",
                        LaunchProgress::Downloading {
                            current: i + 1,
                            total,
                            id: *id,
                            name: name.clone(),
                            bytes,
                            total_bytes: sizes.get(id).copied(),
                        },
                    )
                    .ok();
                    if bytes != last_bytes {
                        last_bytes = bytes;
                        last_progress = std::time::Instant::now();
                    }
                    if bytes == 0 && start.elapsed() > START_TIMEOUT {
                        return Err(cmd_err(
                            "steam_not_downloading",
                            format!(
                                "Steam didn't start downloading mod {id}. Make sure Steam is \
                                 running and logged in, then try again."
                            ),
                            None,
                        ));
                    }
                    if last_progress.elapsed() > STALL_TIMEOUT {
                        return Err(cmd_err(
                            "download_stalled",
                            format!(
                                "Download of mod {id} stalled. Check Steam for a paused or failed \
                                 download (or free disk space), then try again."
                            ),
                            None,
                        ));
                    }
                }
            }
        }
        // `is_download_complete` already confirmed `content/<id>/meta.cpp` exists, so the item is
        // fully on disk in the same `workshop` tree we link from below.
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

/// Diagnostics for the Settings tab: app version, Steam client state, and Steam/DayZ install.
#[derive(Debug, serde::Serialize)]
pub struct EnvReport {
    pub app_version: String,
    /// Whether the Steam client is running — mod downloads are driven through it.
    pub steam_running: bool,
    pub steam_found: bool,
    pub steam_kind: Option<String>,
    pub steam_root: Option<String>,
    pub dayz_installed: bool,
    pub dayz_path: Option<String>,
    pub dayz_version: Option<String>,
}

/// Probe the environment so the user can see whether everything needed to install mods and
/// launch is present. Infallible — missing pieces are reported as `false`/`None`.
#[tauri::command]
pub async fn check_environment(app: tauri::AppHandle) -> EnvReport {
    let runner = RealRunner::new();
    let steam_is_running = steam_running(&runner).await;
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
        steam_running: steam_is_running,
        steam_found,
        steam_kind,
        steam_root,
        dayz_installed,
        dayz_path,
        dayz_version,
    }
}

/// A document-portal FUSE path: `/run/user/<uid>/doc/...` (what the file chooser hands back) or the
/// sandbox-internal `/run/flatpak/doc/...`.
fn is_document_portal_path(path: &str) -> bool {
    path.starts_with("/run/flatpak/doc/")
        || path
            .strip_prefix("/run/user/")
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(_uid, rest)| rest.starts_with("doc/"))
}

/// Turn a folder the user picked into a stored `steam_root`. In a Flatpak the file chooser can't
/// reach a library on e.g. `/mnt`, so the document portal re-exports it under
/// `/run/user/<uid>/doc/<id>/...` and hands back that portal path — per-session and useless as a
/// persisted override. Recover the real host path from the `user.document-portal.host-path` xattr
/// the portal FUSE exposes, then normalize to a Steam library root (the picker usually lands on the
/// game dir). Outside a sandbox — or when the attr is absent — the input is just normalized.
#[tauri::command]
pub fn resolve_dayz_path(path: String) -> String {
    let resolved = if is_document_portal_path(&path) {
        xattr::get(&path, "user.document-portal.host-path")
            .ok()
            .flatten()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or(path)
    } else {
        path
    };
    library_root(std::path::Path::new(&resolved))
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dayz_core::Error;

    #[test]
    fn detects_document_portal_paths() {
        assert!(is_document_portal_path(
            "/run/user/1000/doc/kByBWbtRKRkn6SMgk1cpNg/DayZ"
        ));
        assert!(is_document_portal_path("/run/flatpak/doc/abc123/DayZ"));
        assert!(!is_document_portal_path("/mnt/FAST/SteamLibrary"));
        assert!(!is_document_portal_path("/run/user/1000/foo/bar"));
    }

    #[test]
    fn maps_steam_not_found_and_dayz_not_found() {
        assert_eq!(
            to_command_error(&Error::SteamNotFound).kind,
            "steam_not_found"
        );
        assert_eq!(
            to_command_error(&Error::DayzNotFound).kind,
            "dayz_not_found"
        );
    }
}
