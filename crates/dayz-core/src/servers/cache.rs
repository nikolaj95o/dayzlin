use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::error::Error;
use crate::servers::Server;

pub fn cache_write(path: &Path, servers: &[Server]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec(servers)?)?;
    Ok(())
}

pub fn cache_read(path: &Path, ttl_secs: u64) -> Option<Vec<Server>> {
    let meta = std::fs::metadata(path).ok()?;
    let age = SystemTime::now().duration_since(meta.modified().ok()?).ok()?;
    if age > Duration::from_secs(ttl_secs) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::{Server, ServerMod};
    use tempfile::tempdir;

    fn one() -> Vec<Server> {
        vec![Server {
            name: "A".into(),
            ip: "1.1.1.1".into(),
            game_port: 2302,
            players: 1,
            max_players: 60,
            map: "x".into(),
            time: "12:00".into(),
            first_person: true,
            password: false,
            mods: vec![ServerMod {
                name: "m".into(),
                workshop_id: 1,
            }],
        }]
    }

    #[test]
    fn write_then_read_within_ttl() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("servers.json");
        cache_write(&p, &one()).unwrap();
        let got = cache_read(&p, 300).expect("fresh cache");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "A");
    }

    #[test]
    fn read_returns_none_when_stale() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("servers.json");
        cache_write(&p, &one()).unwrap();
        assert!(cache_read(&p, 0).is_none());
    }

    #[test]
    fn read_returns_none_when_missing() {
        let dir = tempdir().unwrap();
        assert!(cache_read(&dir.path().join("nope.json"), 300).is_none());
    }
}
