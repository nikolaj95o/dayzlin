use std::path::PathBuf;
use std::sync::Mutex;

use dayz_core::launch::launch;
use dayz_core::mods::{
    delete_installed_mod as core_delete_installed_mod, dir_size_bytes, ensure_mod_symlinks,
    is_download_complete, lowercase_mod_tree, missing_mods, remove_workshop_download,
    scan_installed_mods, scan_installed_mods_detailed, workshop_download_url, InstalledModInfo,
};
use dayz_core::process::{open_uri, steam_channel, steam_command_or_uri, steam_running};
use dayz_core::profile::{Profile, ServerRef};
use dayz_core::servers::{
    apply_filter, cache_read, cache_write, dedupe_by_endpoint, fetch_mod_sizes, fetch_servers,
    fuzzy_search, Server, ServerFilter,
};
use dayz_core::steam::{
    app_launch_ready, dayz_app_state, granted_level, library_claiming_dayz, library_root,
    locate_dayz, GrantLevel, SteamInstall,
};
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

/// Current wall-clock time as unix seconds (0 if the clock is before the epoch — not expected).
fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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

    if steam_running(&home()) {
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

    // Resolve once how to talk to Steam for every download and the launch below: native → fire the
    // `steam` binary directly; sandboxed → forward through Steam's pipe helper; otherwise → portal
    // `open_uri` fallback. All to skip the MIME chooser / `steam://run` confirmation where we can.
    let steam_ch = steam_channel(&home());

    let installed = scan_installed_mods(&workshop);
    let missing = missing_mods(&server.mods, &installed);

    // We don't preflight "is Steam running?": each mod download fires a `steam://` URL, which
    // cold-starts Steam if needed, and the launch does the same — so nothing blocks the user. If
    // Steam is down (or logged out) the download loop below surfaces it as a start/stall timeout
    // with actionable guidance.

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
        steam_command_or_uri(&steam_ch, &[&url], &url).map_err(|e| to_command_error(&e))?;

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

    launch(&steam_ch, server, &profile.player, password.as_deref())
        .map_err(|e| to_command_error(&e))?;

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
    // Stamp every mod this server uses as just-used, so the Installed Mods tab can show "Last used".
    // `ids` is the full workshop-id list computed above for symlinking.
    profile.record_mods_used(&ids, now_unix_secs());
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
    /// The library `libraryfolders.vdf` says owns DayZ, when we can't otherwise reach it (e.g. an
    /// off-mount library the sandbox can't stat). Lets the UI offer a one-click "grant access to
    /// <path>" that opens the file chooser pre-aimed there. `None` when DayZ isn't in any vdf.
    pub dayz_library_hint: Option<String>,
}

/// Probe the environment so the user can see whether everything needed to install mods and
/// launch is present. Infallible — missing pieces are reported as `false`/`None`.
#[tauri::command]
pub async fn check_environment(app: tauri::AppHandle) -> EnvReport {
    let steam_is_running = steam_running(&home());
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
                // Show the real host path, not the raw `/run/user/.../doc/...` portal mount.
                Some(display_path(&d.game_dir())),
                read_installed_version(&d.game_dir()),
            ),
            Err(_) => (false, None, None),
        };

    // The exact library folder to grant when DayZ is known to Steam but unreachable from here.
    let dayz_library_hint = library_claiming_dayz(&home()).map(|p| p.display().to_string());

    EnvReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        steam_running: steam_is_running,
        steam_found,
        steam_kind,
        steam_root,
        dayz_installed,
        dayz_path,
        dayz_version,
        dayz_library_hint,
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

/// The real host path a document-portal mount points at (its `user.document-portal.host-path`
/// xattr). Readable on the FUSE mount even when the target itself lies outside the sandbox, so it
/// works for an off-mount library we can't otherwise stat. `None` when the xattr is absent.
fn portal_host_path(path: &str) -> Option<String> {
    xattr::get(path, "user.document-portal.host-path")
        .ok()
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Resolve a stored path to what the user should see: a document-portal path shows its real host
/// target (e.g. `/mnt/FAST/SteamLibrary/...` rather than `/run/user/1000/doc/<id>/...`); anything
/// else is shown unchanged.
fn display_path(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if is_document_portal_path(&s) {
        portal_host_path(&s).unwrap_or_else(|| s.clone().into_owned())
    } else {
        s.into_owned()
    }
}

/// Outcome of turning a picked folder into a stored `steam_root`. `ok` says whether it's a usable
/// DayZ library; `display_path` is the real host path to show the user; `message` explains a
/// rejection so the UI can guide the user to re-pick the right folder.
#[derive(Debug, serde::Serialize)]
pub struct ResolvedDayzPath {
    pub steam_root: String,
    pub display_path: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Turn a folder the user picked into a stored `steam_root`.
///
/// The file chooser hands back a real path for locations we can already reach — default-location
/// Steam libraries, covered by our narrow `:rw` grants — and a document-portal path
/// (`/run/user/<uid>/doc/<id>/...`) for anything else (e.g. a library on `/mnt`). An off-mount
/// library is *only* reachable through that portal grant, so we keep the portal path as
/// `steam_root` and operate through it (rw to exactly that subtree); the real host path from the
/// `user.document-portal.host-path` xattr is used purely for display and messaging. We validate the
/// pick *through the mount* (`<path>/steamapps/common/DayZ`) — the mount is the only place the
/// files are reachable — and reject a pick that's too deep (the game/`common`/`steamapps` folder),
/// since a portal grant can't be climbed back up to the library root.
#[tauri::command]
pub fn resolve_dayz_path(path: String) -> ResolvedDayzPath {
    if is_document_portal_path(&path) {
        let host = portal_host_path(&path).unwrap_or_else(|| path.clone());
        if std::path::Path::new(&path)
            .join("steamapps/common/DayZ")
            .is_dir()
        {
            return ResolvedDayzPath {
                steam_root: path,
                display_path: host,
                ok: true,
                message: None,
            };
        }
        let message = match granted_level(std::path::Path::new(&host)) {
            GrantLevel::TooDeep => format!(
                "You picked a folder inside the Steam library. Pick the library folder that \
                 contains `steamapps` instead — e.g. {}.",
                library_root(std::path::Path::new(&host)).display()
            ),
            GrantLevel::LibraryRoot => "That folder isn't a DayZ Steam library (no \
                 steamapps/common/DayZ inside). Pick the Steam library folder that contains DayZ."
                .to_string(),
        };
        return ResolvedDayzPath {
            steam_root: String::new(),
            display_path: host,
            ok: false,
            message: Some(message),
        };
    }
    // A path we can already reach (default-location library or manual entry): normalize it to the
    // library root and trust the diagnostics probe to report whether DayZ is actually there.
    let root = library_root(std::path::Path::new(&path))
        .to_string_lossy()
        .into_owned();
    ResolvedDayzPath {
        steam_root: root.clone(),
        display_path: root,
        ok: true,
        message: None,
    }
}

/// List every installed workshop mod with its on-disk size and last-used time, for the Mods tab.
#[tauri::command]
pub fn list_installed_mods(app: tauri::AppHandle) -> Result<Vec<InstalledModInfo>, CommandError> {
    let profile = Profile::load(&data_dir(&app).join("profile.json"));
    let dayz =
        locate_dayz(&home(), profile.steam_root.as_deref()).map_err(|e| to_command_error(&e))?;
    Ok(scan_installed_mods_detailed(
        &dayz.workshop_dir(),
        &profile.mod_last_used,
    ))
}

/// Delete an installed mod (workshop content + `@<id>` game symlink) and forget its last-used entry.
#[tauri::command]
pub fn delete_installed_mod(id: u64, app: tauri::AppHandle) -> Result<(), CommandError> {
    let path = data_dir(&app).join("profile.json");
    let mut profile = Profile::load(&path);
    let dayz =
        locate_dayz(&home(), profile.steam_root.as_deref()).map_err(|e| to_command_error(&e))?;
    core_delete_installed_mod(&dayz.workshop_dir(), &dayz.game_dir(), id)
        .map_err(|e| to_command_error(&e))?;
    if profile.mod_last_used.remove(&id).is_some() {
        let _ = profile.save(&path);
    }
    Ok(())
}

/// Open a mod's Steam Workshop page inside the Steam client (cold-starts Steam if it's closed).
#[tauri::command]
pub fn open_workshop_page(id: u64) -> Result<(), CommandError> {
    let url = format!("steam://url/CommunityFilePage/{id}");
    let ch = steam_channel(&home());
    steam_command_or_uri(&ch, &[&url], &url).map_err(|e| to_command_error(&e))
}

/// Open a mod's install directory in the user's file manager.
#[tauri::command]
pub fn open_mod_folder(id: u64, app: tauri::AppHandle) -> Result<(), CommandError> {
    let profile = Profile::load(&data_dir(&app).join("profile.json"));
    let dayz =
        locate_dayz(&home(), profile.steam_root.as_deref()).map_err(|e| to_command_error(&e))?;
    let path = dayz.workshop_dir().join(id.to_string());
    // A file:// URI so the runtime's portal-backed `xdg-open` hands it to OpenURI (opens the host
    // file manager) without any host-spawn permission.
    open_uri(&format!("file://{}", path.display())).map_err(|e| to_command_error(&e))
}

// ---------------------------------------------------------------------------
// App self-update
//
// Two package formats, two mechanisms:
//   * AppImage — `tauri-plugin-updater` downloads a signed replacement from the GitHub release
//     (verified against the embedded pubkey) and swaps the running image.
//   * Flatpak — the `org.freedesktop.portal.Flatpak` UpdateMonitor deploys the update from the
//     app's OSTree remote. This is the sandbox-correct path: it needs no host-spawn / `flatpak`
//     access, so it doesn't undo the manifest's deliberately narrow permissions.
// A plain/dev binary reports backend "none" and can't self-update.
//
// The *availability* check (current vs. latest) is shared: it hits the GitHub "latest release"
// API for both formats. The gh-pages Flatpak repo and the GitHub tag are stamped from the same CI
// version, so the number lines up regardless of how the user installed the app.
// ---------------------------------------------------------------------------

/// Which packaging the running app was launched from, deciding how (and whether) it can update.
fn detect_backend() -> &'static str {
    if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else if std::path::Path::new("/.flatpak-info").exists()
        || std::env::var_os("FLATPAK_ID").is_some()
    {
        "flatpak"
    } else {
        "none"
    }
}

/// Break a dotted version ("0.1.10") into numeric components for ordered comparison. Any
/// non-numeric suffix on a component is ignored, and `Vec<u64>` compares lexicographically, so
/// 0.1.10 > 0.1.3 as expected.
fn parse_ver(v: &str) -> Vec<u64> {
    v.split('.')
        .map(|p| {
            p.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

/// The tag of the newest GitHub release, with any leading `v` stripped (e.g. "0.1.4").
async fn github_latest_version() -> Result<String, CommandError> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }
    let net = |e: reqwest::Error| {
        cmd_err(
            "network",
            "Couldn't reach GitHub to check for updates.",
            Some(e.to_string()),
        )
    };
    let rel: Release = reqwest::Client::new()
        .get("https://api.github.com/repos/nikolaj95o/dayzlin/releases/latest")
        // GitHub rejects requests without a User-Agent.
        .header(reqwest::header::USER_AGENT, "dayzlin-updater")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(net)?
        .json()
        .await
        .map_err(net)?;
    Ok(rel.tag_name.trim_start_matches('v').to_string())
}

/// Update availability + how (if at all) it can be applied; mirrors the TS `UpdateStatus`.
#[derive(serde::Serialize)]
pub struct UpdateStatus {
    /// "appimage" | "flatpak" | "none".
    pub backend: String,
    pub current_version: String,
    /// Newest published version, or `None` if GitHub was unreachable.
    pub latest_version: Option<String>,
    pub update_available: bool,
    /// Whether `install_update` can actually apply it (false for a plain/dev binary).
    pub apply_supported: bool,
}

/// Report whether a newer release exists. Infallible — a network failure just leaves
/// `latest_version` unset (no update offered) rather than surfacing an error, so the startup
/// auto-check stays quiet.
#[tauri::command]
pub async fn check_for_updates() -> UpdateStatus {
    let backend = detect_backend();
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = github_latest_version().await.ok();
    let update_available = latest
        .as_deref()
        .is_some_and(|l| parse_ver(l) > parse_ver(&current));
    UpdateStatus {
        backend: backend.to_string(),
        current_version: current,
        latest_version: latest,
        update_available,
        apply_supported: backend != "none",
    }
}

/// Download, apply, and restart into the newest release. Diverges (restarts the process) on
/// success; returns an error the frontend can show otherwise.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), CommandError> {
    match detect_backend() {
        "appimage" => appimage_update(&app).await,
        "flatpak" => flatpak_update(&app).await,
        _ => Err(cmd_err(
            "update",
            "This build can't update itself. Download the latest AppImage from the releases page.",
            None,
        )),
    }
}

/// AppImage: let `tauri-plugin-updater` verify + swap the image, then restart into it.
async fn appimage_update(app: &tauri::AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_updater::UpdaterExt;
    let up = |msg: &str, e: tauri_plugin_updater::Error| cmd_err("update", msg, Some(e.to_string()));
    let update = app
        .updater()
        .map_err(|e| up("The updater is unavailable.", e))?
        .check()
        .await
        .map_err(|e| up("Couldn't check for an update.", e))?
        .ok_or_else(|| cmd_err("update", "No update is available.", None))?;
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| up("Failed to download or install the update.", e))?;
    app.restart();
}

fn portal_err(e: zbus::Error) -> CommandError {
    cmd_err(
        "update",
        "Couldn't talk to the Flatpak update portal.",
        Some(e.to_string()),
    )
}

/// Flatpak: ask the portal to deploy the update from our OSTree remote, then restart into it.
async fn flatpak_update(app: &tauri::AppHandle) -> Result<(), CommandError> {
    use futures_util::StreamExt;

    let conn = zbus::Connection::session().await.map_err(portal_err)?;
    let portal = FlatpakPortalProxy::new(&conn).await.map_err(portal_err)?;
    let monitor_path = portal
        .create_update_monitor(std::collections::HashMap::new())
        .await
        .map_err(portal_err)?;
    let monitor = UpdateMonitorProxy::builder(&conn)
        .path(monitor_path)
        .map_err(portal_err)?
        .build()
        .await
        .map_err(portal_err)?;

    // Subscribe before triggering so the first progress signal can't race past us.
    let mut progress = monitor
        .receive_update_progress()
        .await
        .map_err(portal_err)?;
    monitor
        .update("", std::collections::HashMap::new())
        .await
        .map_err(portal_err)?;

    // Drive to a terminal status: 0 running, 1 empty (nothing to deploy), 2 done, 3 error.
    loop {
        let sig = tokio::time::timeout(std::time::Duration::from_secs(600), progress.next())
            .await
            .map_err(|_| cmd_err("update", "Timed out waiting for the Flatpak update.", None))?;
        let Some(sig) = sig else {
            return Err(cmd_err(
                "update",
                "The Flatpak update monitor closed unexpectedly.",
                None,
            ));
        };
        let info = sig.args().map_err(portal_err)?.info;
        let status = info
            .get("status")
            .and_then(|v| u32::try_from(v.clone()).ok())
            .unwrap_or(0);
        match status {
            0 => continue,
            2 => break,
            1 => {
                let _ = monitor.close().await;
                return Err(cmd_err(
                    "update",
                    "The Flatpak remote has no update available yet. Try again shortly.",
                    None,
                ));
            }
            _ => {
                let detail = info
                    .get("error_message")
                    .and_then(|v| String::try_from(v.clone()).ok());
                let _ = monitor.close().await;
                return Err(cmd_err("update", "The Flatpak update failed.", detail));
            }
        }
    }
    let _ = monitor.close().await;
    app.restart();
}

/// `org.freedesktop.portal.Flatpak` — creating the per-app update monitor.
#[zbus::proxy(
    interface = "org.freedesktop.portal.Flatpak",
    default_service = "org.freedesktop.portal.Flatpak",
    default_path = "/org/freedesktop/portal/Flatpak"
)]
trait FlatpakPortal {
    fn create_update_monitor(
        &self,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

/// The monitor object returned above: trigger an update and watch its progress.
#[zbus::proxy(
    interface = "org.freedesktop.portal.Flatpak.UpdateMonitor",
    default_service = "org.freedesktop.portal.Flatpak"
)]
trait UpdateMonitor {
    fn update(
        &self,
        parent_window: &str,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    fn close(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn update_progress(
        &self,
        info: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::Result<()>;
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
