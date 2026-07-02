use std::path::Path;

use crate::error::Error;

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
