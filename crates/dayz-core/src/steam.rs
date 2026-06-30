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
/// (the directory containing `steamapps`, e.g. `/mnt/FAST/SteamLibrary`), which is also the
/// `+force_install_dir` base handed to SteamCMD so downloaded content lands in this same tree.
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

    /// Staging directory SteamCMD writes an in-progress workshop download into (before moving the
    /// finished item into [`workshop_dir`]). Polling its size gives live download progress.
    pub fn workshop_downloads_dir(&self) -> PathBuf {
        self.root
            .join("steamapps/workshop/downloads")
            .join(DAYZ_APP_ID.to_string())
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

/// Scan removable/extra mount points for a Steam library containing DayZ. This is the last
/// resort for libraries Steam has "orphaned" — present on disk but listed in no `*.vdf`. For each
/// base (e.g. `/mnt`) we look at each mounted drive and one level of children, so both
/// `<base>/<drive>/steamapps/...` and `<base>/<drive>/SteamLibrary/steamapps/...` are found.
/// Bounded to that depth and to cheap `is_dir` checks, so it stays fast.
fn scan_mounts_for_dayz(bases: &[PathBuf]) -> Vec<PathBuf> {
    let has_dayz = |root: &Path| root.join("steamapps/common/DayZ").is_dir();
    let mut found = Vec::new();
    for base in bases {
        let Ok(drives) = fs::read_dir(base) else {
            continue;
        };
        for drive in drives.flatten().map(|e| e.path()) {
            if has_dayz(&drive) {
                found.push(drive.clone());
            }
            let Ok(children) = fs::read_dir(&drive) else {
                continue;
            };
            for child in children.flatten().map(|e| e.path()) {
                if has_dayz(&child) {
                    found.push(child);
                }
            }
        }
    }
    found
}

/// Default mount bases to scan for orphaned Steam libraries. `$USER` is derived from `home`'s
/// final component so this matches the running user's removable-media paths.
fn default_mount_bases(home: &Path) -> Vec<PathBuf> {
    let mut bases = vec![PathBuf::from("/mnt"), PathBuf::from("/media")];
    if let Some(user) = home.file_name() {
        bases.push(Path::new("/media").join(user));
        bases.push(Path::new("/run/media").join(user));
    }
    bases
}

/// Locate a DayZ install across all Steam libraries. Resolution order: a non-empty
/// `override_root` (the user-configured library folder) wins when it contains DayZ; then every
/// library from `library_candidates` (vdf + default Steam roots); then a scan of mounted drives
/// for orphaned libraries Steam no longer tracks. Errors with `DayzNotFound` if none contain DayZ.
pub fn locate_dayz(home: &Path, override_root: Option<&str>) -> Result<DayzInstall, Error> {
    locate_dayz_in(home, override_root, &default_mount_bases(home))
}

/// Inner implementation of [`locate_dayz`] with injectable mount `bases`, so tests can scan a
/// tempdir instead of the real `/mnt`.
fn locate_dayz_in(
    home: &Path,
    override_root: Option<&str>,
    mount_bases: &[PathBuf],
) -> Result<DayzInstall, Error> {
    let has_dayz = |root: &Path| root.join("steamapps/common/DayZ").is_dir();

    if let Some(ov) = override_root.map(str::trim).filter(|s| !s.is_empty()) {
        let root = PathBuf::from(ov);
        if has_dayz(&root) {
            return Ok(DayzInstall { root });
        }
    }
    for root in library_candidates(home) {
        if has_dayz(&root) {
            return Ok(DayzInstall { root });
        }
    }
    if let Some(root) = scan_mounts_for_dayz(mount_bases).into_iter().next() {
        return Ok(DayzInstall { root });
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
                                                       // Empty mount bases so the scan can't reach the real `/mnt`.
        assert!(matches!(
            locate_dayz_in(home.path(), None, &[]),
            Err(crate::Error::DayzNotFound)
        ));
    }

    #[test]
    fn locate_falls_through_to_mount_scan() {
        let home = tempdir().unwrap(); // no vdf, no DayZ in defaults
        let base = tempdir().unwrap();
        let lib = base.path().join("FAST/SteamLibrary");
        make_dayz(&lib);
        let dayz = locate_dayz_in(home.path(), None, &[base.path().to_path_buf()]).unwrap();
        assert_eq!(dayz.root, lib);
    }

    #[test]
    fn scans_mounts_for_orphaned_library() {
        // Mimic `/mnt/FAST/SteamLibrary/steamapps/common/DayZ`.
        let base = tempdir().unwrap();
        let lib = base.path().join("FAST/SteamLibrary");
        make_dayz(&lib);
        let found = scan_mounts_for_dayz(&[base.path().to_path_buf()]);
        assert_eq!(found, vec![lib]);
    }

    #[test]
    fn scan_finds_library_at_drive_root() {
        // Library sits directly on the drive: `/mnt/DRIVE/steamapps/common/DayZ`.
        let base = tempdir().unwrap();
        let drive = base.path().join("DRIVE");
        make_dayz(&drive);
        let found = scan_mounts_for_dayz(&[base.path().to_path_buf()]);
        assert_eq!(found, vec![drive]);
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
