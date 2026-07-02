use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::steam::{SteamInstall, SteamKind};

/// Open a URI (`steam://…`, `file://…`) with the host's handler. Fire-and-forget.
///
/// This deliberately runs the runtime's own `xdg-open` *inside* the sandbox rather than
/// `flatpak-spawn --host …`: in a Flatpak, `xdg-open` is the portal-backed shim that hands the URI
/// to `org.freedesktop.portal.OpenURI`, so Steam (the host handler for `steam://`) launches without
/// the app holding `--talk-name=org.freedesktop.Flatpak` or any host-spawn permission. On a plain
/// host it's just the usual `xdg-open`. Steam auto-starts if it isn't already running.
pub fn open_uri(uri: &str) -> Result<(), Error> {
    std::process::Command::new("xdg-open")
        .arg(uri)
        .spawn()
        .map(|_| ())
        .map_err(Error::Io)
}

/// True when we're running inside a Flatpak sandbox, where we can't spawn the host Steam directly
/// and must route through the portal-backed [`open_uri`]. `/.flatpak-info` is the canonical marker
/// the runtime mounts into every Flatpak app.
pub fn is_sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// How to hand Steam a command, resolved once from the environment + detected install. Both the
/// `Direct` and `Pipe` routes deliver the *CLI form* of a command (`-applaunch …`), which skips the
/// desktop MIME chooser and Steam's `steam://run` command-confirmation dialog; `Portal` is the
/// `xdg-open` fallback that may prompt.
#[derive(Debug, Clone)]
pub enum SteamChannel {
    /// Native dayzlin: run the `steam` / `flatpak run com.valvesoftware.Steam` binary directly.
    Direct(SteamKind),
    /// Sandboxed dayzlin: forward to a running Steam through its command pipe by executing Steam's
    /// own `steam-runtime-steam-remote` helper (under this Steam root) inside the sandbox — no
    /// host-spawn permission needed.
    Pipe(PathBuf),
    /// No direct route available: open the `steam://` URL via the portal-backed [`open_uri`].
    Portal,
}

/// Resolve how to reach Steam. Native dayzlin talks to the Steam binary directly. A sandboxed
/// dayzlin can't spawn host Steam, so it forwards through Steam's pipe helper — but only when Steam
/// is actually running (the helper no-ops otherwise) and the helper is reachable; anything else
/// (Steam down, helper missing, no install detected) falls back to the portal.
pub fn steam_channel(home: &Path) -> SteamChannel {
    match SteamInstall::detect_in(home) {
        Ok(install) if is_sandboxed() => {
            if steam_running(home) && steam_remote_bin(&install.root).exists() {
                SteamChannel::Pipe(install.root)
            } else {
                SteamChannel::Portal
            }
        }
        Ok(install) => SteamChannel::Direct(install.kind),
        Err(_) => SteamChannel::Portal,
    }
}

/// Steam's bundled command forwarder, which writes to `~/.steam/steam.pipe` in the client's own
/// wire format — the same binary `steam`'s `bin_steam.sh` calls from `forward_command_line`. It
/// lives under the Steam data root and resolves its glib deps via a relative rpath there, so it runs
/// inside our sandbox using only the runtime's stock libc.
fn steam_remote_bin(root: &Path) -> PathBuf {
    root.join("ubuntu12_32/steam-runtime/amd64/usr/bin/steam-runtime-steam-remote")
}

/// Forward `args` to a running Steam via [`steam_remote_bin`]. Returns `true` only if the helper
/// spawned and exited 0 (Valve's own success signal); any failure — helper won't exec, no pipe
/// reader — returns `false` so the caller falls back to the portal. Blocking is fine: the helper
/// writes to the pipe and exits immediately (it doesn't hold the game).
fn forward_via_steam_pipe(root: &Path, args: &[&str]) -> bool {
    std::process::Command::new(steam_remote_bin(root))
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Program + args to invoke Steam directly for a given install kind. Native Steam is the `steam`
/// binary on `PATH`; a Flatpak Steam is reached via `flatpak run com.valvesoftware.Steam` (handles
/// a native dayzlin driving a Flatpak-installed Steam).
fn steam_command(kind: SteamKind, args: &[&str]) -> (&'static str, Vec<String>) {
    match kind {
        SteamKind::Native => ("steam", args.iter().map(|s| s.to_string()).collect()),
        SteamKind::Flatpak => {
            let mut v = vec!["run".to_string(), "com.valvesoftware.Steam".to_string()];
            v.extend(args.iter().map(|s| s.to_string()));
            ("flatpak", v)
        }
    }
}

/// Fire the Steam client directly with `args` (fire-and-forget). Used on native dayzlin to bypass
/// `xdg-open`: naming the binary skips the desktop MIME chooser, and passing `-applaunch …` as CLI
/// args skips Steam's `steam://run` command-confirmation dialog.
pub fn spawn_steam(kind: SteamKind, args: &[&str]) -> Result<(), Error> {
    let (program, args) = steam_command(kind, args);
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(Error::Io)
}

/// Send Steam a CLI-form command (`args`) via the resolved `channel`, falling back to opening
/// `fallback_uri` through the portal when the direct/pipe route is unavailable or fails. Used for
/// every `steam://` action: launch (`args` = `-applaunch …`, `fallback_uri` = the `steam://run`
/// URL) and workshop download / page (`args` = `[uri]`, `fallback_uri` = the same URI).
pub fn steam_command_or_uri(
    channel: &SteamChannel,
    args: &[&str],
    fallback_uri: &str,
) -> Result<(), Error> {
    match channel {
        SteamChannel::Direct(kind) => spawn_steam(*kind, args).or_else(|_| open_uri(fallback_uri)),
        SteamChannel::Pipe(root) if forward_via_steam_pipe(root, args) => Ok(()),
        _ => open_uri(fallback_uri),
    }
}

/// True if the Steam client appears to be running, determined without host process access so it
/// works the same inside a Flatpak sandbox as on the host.
///
/// Steam listens for CLI commands on a FIFO at `~/.steam/steam.pipe` (Flatpak Steam: under its
/// per-app home). While Steam is up it holds that FIFO open for reading, so opening the *write* end
/// non-blocking succeeds; with no reader the kernel returns `ENXIO`. Opening and immediately closing
/// the write end sends no data — exactly what every `steam …` CLI invocation does — so it's a
/// harmless probe. We only need this to gate *sensitive edits* of Steam-owned files (see
/// [`crate::mods::remove_workshop_download`]); the launch/download paths just fire `steam://` URLs,
/// which start Steam on their own.
pub fn steam_running(home: &Path) -> bool {
    steam_pipe_candidates(home)
        .iter()
        .any(|p| pipe_has_reader(p) == Some(true))
}

fn steam_pipe_candidates(home: &Path) -> [std::path::PathBuf; 2] {
    [
        home.join(".steam/steam.pipe"),
        home.join(".var/app/com.valvesoftware.Steam/.steam/steam.pipe"),
    ]
}

/// `Some(true)` if a reader (Steam) holds the FIFO open, `Some(false)` if the FIFO exists but has no
/// reader, `None` if it's missing or can't be opened (unknown).
fn pipe_has_reader(path: &Path) -> Option<bool> {
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(_) => Some(true),
        Err(e) if e.raw_os_error() == Some(libc::ENXIO) => Some(false),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::fs::OpenOptionsExt;

    fn mkfifo(path: &Path) {
        let c = CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    }

    #[test]
    fn pipe_reader_detection() {
        let dir = tempfile::tempdir().unwrap();

        // Missing FIFO => unknown.
        assert_eq!(pipe_has_reader(&dir.path().join("nope.pipe")), None);

        let pipe = dir.path().join("steam.pipe");
        mkfifo(&pipe);

        // No reader attached => Some(false).
        assert_eq!(pipe_has_reader(&pipe), Some(false));

        // Hold the read end open (as a running Steam would) => Some(true).
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&pipe)
            .unwrap();
        assert_eq!(pipe_has_reader(&pipe), Some(true));
        drop(reader);
        assert_eq!(pipe_has_reader(&pipe), Some(false));
    }

    #[test]
    fn steam_remote_bin_sits_under_the_steam_data_root() {
        let root = Path::new("/home/me/.local/share/Steam");
        assert_eq!(
            steam_remote_bin(root),
            root.join("ubuntu12_32/steam-runtime/amd64/usr/bin/steam-runtime-steam-remote")
        );
    }

    #[test]
    fn steam_command_native_uses_bare_steam_binary() {
        let (program, args) = steam_command(SteamKind::Native, &["steam://url/CommunityFilePage/1"]);
        assert_eq!(program, "steam");
        assert_eq!(args, vec!["steam://url/CommunityFilePage/1"]);
    }

    #[test]
    fn steam_command_flatpak_wraps_in_flatpak_run() {
        let (program, args) = steam_command(SteamKind::Flatpak, &["-applaunch", "221100"]);
        assert_eq!(program, "flatpak");
        assert_eq!(
            args,
            vec!["run", "com.valvesoftware.Steam", "-applaunch", "221100"]
        );
    }

    #[test]
    fn steam_running_checks_native_pipe() {
        let home = tempfile::tempdir().unwrap();
        let pipe = home.path().join(".steam/steam.pipe");
        std::fs::create_dir_all(pipe.parent().unwrap()).unwrap();
        mkfifo(&pipe);

        assert!(!steam_running(home.path()));
        let _reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&pipe)
            .unwrap();
        assert!(steam_running(home.path()));
    }
}
