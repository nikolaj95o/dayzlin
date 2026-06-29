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

    pub fn workshop_dir(&self) -> PathBuf {
        self.root
            .join("steamapps/workshop/content")
            .join(DAYZ_APP_ID.to_string())
    }

    pub fn game_dir(&self) -> PathBuf {
        self.root.join("steamapps/common/DayZ")
    }
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

    #[test]
    fn detects_native_steam() {
        let home = tempdir().unwrap();
        make_steam(home.path(), ".steam/steam");
        let s = SteamInstall::detect_in(home.path()).unwrap();
        assert_eq!(s.kind, SteamKind::Native);
        assert!(s
            .workshop_dir()
            .ends_with("steamapps/workshop/content/221100"));
        assert!(s.game_dir().ends_with("steamapps/common/DayZ"));
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
}
