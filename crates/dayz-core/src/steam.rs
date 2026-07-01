use std::fs;
use std::path::{Path, PathBuf};

use crate::{error::Error, DAYZ_APP_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamKind {
    Native,
    Flatpak,
}

#[derive(Debug, Clone)]
pub struct SteamInstall {
    pub kind: SteamKind,
    pub root: PathBuf,
}

impl SteamInstall {
    /// Detect a Steam install under `home`, preferring native over Flatpak.
    pub fn detect_in(home: &Path) -> Result<Self, Error> {
        let candidates = [
            (SteamKind::Native, home.join(".steam/steam")),
            (SteamKind::Native, home.join(".local/share/Steam")),
            (
                SteamKind::Flatpak,
                home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
            ),
        ];
        for (kind, root) in candidates {
            if root.join("steamapps").is_dir() {
                return Ok(Self { kind, root });
            }
        }
        Err(Error::SteamNotFound)
    }
}

/// A Steam library folder that actually contains a DayZ install. `root` is the library root
/// (the directory containing `steamapps`, e.g. `/mnt/FAST/SteamLibrary`); the Steam client
/// downloads workshop content into this tree, which is where we scan for and link mods.
#[derive(Debug, Clone)]
pub struct DayzInstall {
    pub root: PathBuf,
}

impl DayzInstall {
    pub fn game_dir(&self) -> PathBuf {
        self.root.join("steamapps/common/DayZ")
    }

    pub fn workshop_dir(&self) -> PathBuf {
        self.root
            .join("steamapps/workshop/content")
            .join(DAYZ_APP_ID.to_string())
    }

    /// Staging directory the Steam client writes an in-progress workshop download into (before
    /// moving the finished item into [`workshop_dir`]). Polling its size gives live download progress.
    pub fn workshop_downloads_dir(&self) -> PathBuf {
        self.root
            .join("steamapps/workshop/downloads")
            .join(DAYZ_APP_ID.to_string())
    }

    /// The `appworkshop_<appid>.acf` manifest where Steam records workshop install/download state
    /// (`NeedsDownload`, `WorkshopItemsInstalled`, `WorkshopItemDetails`). A download requested via
    /// `+workshop_download_item` — which never subscribes — leaves a pending entry here that
    /// survives cancelling, so cleaning up an unwanted download means editing this file.
    pub fn workshop_manifest_path(&self) -> PathBuf {
        self.root
            .join("steamapps/workshop")
            .join(format!("appworkshop_{DAYZ_APP_ID}.acf"))
    }
}

/// `appmanifest_<appid>.acf` records install state in a `StateFlags` bitfield. A clean, launchable
/// app reads exactly `4` (only [`STATE_FULLY_INSTALLED`]); the `2` ([`STATE_UPDATE_REQUIRED`]) bit
/// means an update/repair is pending, which is what makes `steam -applaunch` fail with errors like
/// "Invalid platform".
pub const STATE_FULLY_INSTALLED: u32 = 4;
pub const STATE_UPDATE_REQUIRED: u32 = 2;

/// True when DayZ is in a state Steam will actually launch: fully installed and not awaiting an
/// update. `flags` is the `StateFlags` value from the appmanifest.
pub fn app_launch_ready(flags: u32) -> bool {
    flags & STATE_FULLY_INSTALLED != 0 && flags & STATE_UPDATE_REQUIRED == 0
}

/// Read DayZ's `StateFlags` from `<root>/steamapps/appmanifest_<DAYZ_APP_ID>.acf`. Uses the same
/// tiny line-scan as [`parse_library_paths`] rather than a full VDF parser. Returns `None` when the
/// manifest is missing or unparseable, so callers never block launch on a parse miss.
pub fn dayz_app_state(root: &Path) -> Option<u32> {
    let manifest = root
        .join("steamapps")
        .join(format!("appmanifest_{DAYZ_APP_ID}.acf"));
    let contents = fs::read_to_string(&manifest).ok()?;
    contents.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("\"StateFlags\"")?;
        let start = rest.find('"')? + 1;
        let end = rest[start..].find('"')? + start;
        rest[start..end].parse::<u32>().ok()
    })
}

/// Extract every `"path"  "<value>"` entry from a `libraryfolders.vdf`. Mirrors dayz-ctl's
/// `grep -Po '"path"\s*"\K([^"]*)'` — a tiny line scan instead of a full VDF parser.
fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("\"path\"")?;
            let start = rest.find('"')? + 1;
            let end = rest[start..].find('"')? + start;
            Some(PathBuf::from(&rest[start..end]))
        })
        .collect()
}

/// Candidate Steam library roots to search for DayZ. Reads the first `libraryfolders.vdf` found
/// among the well-known locations and returns its main library (the dir containing `steamapps`)
/// plus every `"path"` entry. Falls back to the default Steam roots when no VDF exists.
fn library_candidates(home: &Path) -> Vec<PathBuf> {
    let vdf_locations = [
        home.join(".steam/steam/steamapps/libraryfolders.vdf"),
        home.join(".local/share/Steam/steamapps/libraryfolders.vdf"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/libraryfolders.vdf"),
        home.join(".steam/root/config/libraryfolders.vdf"),
    ];
    for vdf in vdf_locations {
        let Ok(contents) = fs::read_to_string(&vdf) else {
            continue;
        };
        let mut roots = Vec::new();
        // The main library is the directory two levels above the vdf (parent of `steamapps`
        // or `config`), i.e. the Steam root that owns this file.
        if let Some(main) = vdf.parent().and_then(Path::parent) {
            roots.push(main.to_path_buf());
        }
        roots.extend(parse_library_paths(&contents));
        return roots;
    }
    vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
    ]
}

/// Normalize a user-supplied path to a Steam *library root* (the dir containing `steamapps`).
/// The folder picker and manual entry often land on a deeper path — the game dir
/// (`<lib>/steamapps/common/DayZ`) or the `<lib>/steamapps` dir — so strip those suffixes back
/// to the library root. Any other path is returned unchanged.
pub fn library_root(path: &Path) -> PathBuf {
    if path.ends_with("steamapps/common/DayZ") {
        // <lib>/steamapps/common/DayZ -> <lib>
        if let Some(lib) = path.parent().and_then(Path::parent).and_then(Path::parent) {
            return lib.to_path_buf();
        }
    }
    if path.ends_with("steamapps") {
        if let Some(lib) = path.parent() {
            return lib.to_path_buf();
        }
    }
    path.to_path_buf()
}

/// Locate a DayZ install across all Steam libraries. Resolution order: a non-empty
/// `override_root` (the user-configured library folder, normalized via [`library_root`]) wins when
/// it contains DayZ; then every library from `library_candidates` (vdf + default Steam roots).
/// Errors with `DayzNotFound` if none contain DayZ. Libraries Steam knows about are always listed
/// in a `libraryfolders.vdf`, so we rely on those files rather than scanning mounted drives.
pub fn locate_dayz(home: &Path, override_root: Option<&str>) -> Result<DayzInstall, Error> {
    let has_dayz = |root: &Path| root.join("steamapps/common/DayZ").is_dir();

    if let Some(ov) = override_root.map(str::trim).filter(|s| !s.is_empty()) {
        let root = library_root(Path::new(ov));
        if has_dayz(&root) {
            return Ok(DayzInstall { root });
        }
    }
    for root in library_candidates(home) {
        if has_dayz(&root) {
            return Ok(DayzInstall { root });
        }
    }
    Err(Error::DayzNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_steam(home: &std::path::Path, rel_root: &str) {
        let root = home.join(rel_root);
        fs::create_dir_all(root.join("steamapps")).unwrap();
    }

    fn make_dayz(lib_root: &std::path::Path) {
        fs::create_dir_all(lib_root.join("steamapps/common/DayZ")).unwrap();
    }

    /// Write a `libraryfolders.vdf` under the native steam root listing the given library paths.
    fn write_vdf(home: &std::path::Path, paths: &[&std::path::Path]) {
        let entries: String = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "\t\"{i}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                    p.display()
                )
            })
            .collect();
        let vdf = format!("\"libraryfolders\"\n{{\n{entries}}}\n");
        let dir = home.join(".steam/steam/steamapps");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("libraryfolders.vdf"), vdf).unwrap();
    }

    #[test]
    fn detects_native_steam() {
        let home = tempdir().unwrap();
        make_steam(home.path(), ".steam/steam");
        let s = SteamInstall::detect_in(home.path()).unwrap();
        assert_eq!(s.kind, SteamKind::Native);
    }

    #[test]
    fn parses_multiple_library_paths() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path"		"/home/me/.local/share/Steam"
    }
    "1"
    {
        "path"		"/mnt/FAST/SteamLibrary"
    }
}
"#;
        let paths = parse_library_paths(vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/me/.local/share/Steam"),
                PathBuf::from("/mnt/FAST/SteamLibrary"),
            ]
        );
    }

    #[test]
    fn locates_dayz_in_secondary_library_from_vdf() {
        let home = tempdir().unwrap();
        let lib = tempdir().unwrap(); // a separate "drive"
        make_dayz(lib.path());
        write_vdf(
            home.path(),
            &[&home.path().join(".local/share/Steam"), lib.path()],
        );

        let dayz = locate_dayz(home.path(), None).unwrap();
        assert_eq!(dayz.root, lib.path());
        assert!(dayz.game_dir().ends_with("steamapps/common/DayZ"));
        assert!(dayz
            .workshop_dir()
            .ends_with("steamapps/workshop/content/221100"));
    }

    #[test]
    fn override_takes_priority() {
        let home = tempdir().unwrap();
        let lib = tempdir().unwrap();
        make_dayz(lib.path());
        // No vdf, no default-location DayZ — only the override has it.
        let dayz = locate_dayz(home.path(), Some(lib.path().to_str().unwrap())).unwrap();
        assert_eq!(dayz.root, lib.path());
    }

    #[test]
    fn override_pointing_at_game_dir_resolves_to_library_root() {
        let home = tempdir().unwrap();
        let lib = tempdir().unwrap();
        make_dayz(lib.path());
        // The folder picker often lands on the game dir; it must resolve back to the library root.
        let game_dir = lib.path().join("steamapps/common/DayZ");
        let dayz = locate_dayz(home.path(), Some(game_dir.to_str().unwrap())).unwrap();
        assert_eq!(dayz.root, lib.path());
    }

    #[test]
    fn library_root_strips_known_suffixes() {
        let lib = PathBuf::from("/mnt/FAST/SteamLibrary");
        assert_eq!(library_root(&lib.join("steamapps/common/DayZ")), lib);
        assert_eq!(library_root(&lib.join("steamapps")), lib);
        assert_eq!(library_root(&lib), lib);
    }

    #[test]
    fn falls_back_to_default_locations_without_vdf() {
        let home = tempdir().unwrap();
        make_dayz(&home.path().join(".local/share/Steam"));
        let dayz = locate_dayz(home.path(), None).unwrap();
        assert_eq!(dayz.root, home.path().join(".local/share/Steam"));
    }

    #[test]
    fn dayz_not_found_when_absent() {
        let home = tempdir().unwrap();
        make_steam(home.path(), ".local/share/Steam"); // steam present, but no DayZ
        assert!(matches!(
            locate_dayz(home.path(), None),
            Err(crate::Error::DayzNotFound)
        ));
    }

    #[test]
    fn detects_flatpak_steam_when_native_absent() {
        let home = tempdir().unwrap();
        make_steam(home.path(), ".var/app/com.valvesoftware.Steam/data/Steam");
        let s = SteamInstall::detect_in(home.path()).unwrap();
        assert_eq!(s.kind, SteamKind::Flatpak);
    }

    #[test]
    fn prefers_native_when_both_present() {
        let home = tempdir().unwrap();
        make_steam(home.path(), ".steam/steam");
        make_steam(home.path(), ".var/app/com.valvesoftware.Steam/data/Steam");
        let s = SteamInstall::detect_in(home.path()).unwrap();
        assert_eq!(s.kind, SteamKind::Native);
    }

    #[test]
    fn errors_when_no_steam() {
        let home = tempdir().unwrap();
        assert!(matches!(
            SteamInstall::detect_in(home.path()),
            Err(crate::Error::SteamNotFound)
        ));
    }

    #[test]
    fn app_launch_ready_only_for_clean_install() {
        assert!(app_launch_ready(STATE_FULLY_INSTALLED)); // 4: installed, no update pending
        assert!(!app_launch_ready(6)); // 4 | 2: installed but update required
        assert!(!app_launch_ready(2)); // update required, not installed
        assert!(!app_launch_ready(0));
    }

    /// Write a minimal `appmanifest_221100.acf` with the given StateFlags under `<root>/steamapps`.
    fn write_appmanifest(root: &std::path::Path, state_flags: &str) {
        let dir = root.join("steamapps");
        fs::create_dir_all(&dir).unwrap();
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"221100\"\n\t\"StateFlags\"\t\t\"{state_flags}\"\n}}\n"
        );
        fs::write(dir.join("appmanifest_221100.acf"), acf).unwrap();
    }

    #[test]
    fn reads_state_flags_from_appmanifest() {
        let root = tempdir().unwrap();
        write_appmanifest(root.path(), "6");
        assert_eq!(dayz_app_state(root.path()), Some(6));
    }

    #[test]
    fn dayz_app_state_none_when_manifest_absent() {
        let root = tempdir().unwrap();
        assert_eq!(dayz_app_state(root.path()), None);
    }
}
