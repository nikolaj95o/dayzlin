use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRef {
    pub name: String,
    pub ip: String,
    pub port: u16,
}

impl ServerRef {
    fn same_endpoint(&self, other: &ServerRef) -> bool {
        self.ip == other.ip && self.port == other.port
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub steam_root: Option<String>,
    #[serde(default)]
    pub favorites: Vec<ServerRef>,
    #[serde(default)]
    pub history: Vec<ServerRef>,
}

impl Profile {
    pub fn load(path: &Path) -> Profile {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn add_history(&mut self, r: ServerRef, limit: usize) {
        self.history.retain(|h| !h.same_endpoint(&r));
        self.history.insert(0, r);
        self.history.truncate(limit);
    }

    pub fn toggle_favorite(&mut self, r: ServerRef) {
        if let Some(pos) = self.favorites.iter().position(|f| f.same_endpoint(&r)) {
            self.favorites.remove(pos);
        } else {
            self.favorites.push(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn r(name: &str, ip: &str) -> ServerRef {
        ServerRef {
            name: name.into(),
            ip: ip.into(),
            port: 2302,
        }
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = tempdir().unwrap();
        let p = Profile::load(&dir.path().join("none.json"));
        assert_eq!(p.player, "");
        assert!(p.favorites.is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let mut p = Profile::default();
        p.player = "survivor".into();
        p.save(&path).unwrap();
        let loaded = Profile::load(&path);
        assert_eq!(loaded.player, "survivor");
    }

    #[test]
    fn add_history_dedups_and_caps() {
        let mut p = Profile::default();
        p.add_history(r("A", "1.1.1.1"), 2);
        p.add_history(r("B", "2.2.2.2"), 2);
        p.add_history(r("A", "1.1.1.1"), 2); // moves A to front
        p.add_history(r("C", "3.3.3.3"), 2); // caps to 2
        assert_eq!(p.history.len(), 2);
        assert_eq!(p.history[0].name, "C");
        assert_eq!(p.history[1].name, "A");
    }

    #[test]
    fn toggle_favorite_adds_then_removes() {
        let mut p = Profile::default();
        p.toggle_favorite(r("A", "1.1.1.1"));
        assert_eq!(p.favorites.len(), 1);
        p.toggle_favorite(r("A", "1.1.1.1"));
        assert!(p.favorites.is_empty());
    }
}
