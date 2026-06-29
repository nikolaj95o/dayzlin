use std::path::Path;

use crate::servers::ServerMod;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InstalledMod {
    pub name: String,
    pub workshop_id: u64,
}

/// Extract name + publishedid from a `meta.cpp` body. Returns None if either is absent.
pub fn parse_meta_cpp(text: &str) -> Option<InstalledMod> {
    let id = field(text, "publishedid")?.parse::<u64>().ok()?;
    let name = field_quoted(text, "name")?;
    Some(InstalledMod {
        name,
        workshop_id: id,
    })
}

fn field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                return Some(rest.trim().trim_end_matches(';').trim().to_string());
            }
        }
    }
    None
}

fn field_quoted(text: &str, key: &str) -> Option<String> {
    let raw = field(text, key)?;
    Some(raw.trim_matches('"').to_string())
}

pub fn scan_installed_mods(workshop_dir: &Path) -> Vec<InstalledMod> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(workshop_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let meta = entry.path().join("meta.cpp");
        if let Ok(text) = std::fs::read_to_string(&meta) {
            if let Some(m) = parse_meta_cpp(&text) {
                out.push(m);
            }
        }
    }
    out
}

pub fn missing_mods(required: &[ServerMod], installed: &[InstalledMod]) -> Vec<u64> {
    required
        .iter()
        .filter(|r| !installed.iter().any(|i| i.workshop_id == r.workshop_id))
        .map(|r| r.workshop_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::ServerMod;
    use std::fs;
    use tempfile::tempdir;

    const META: &str = r#"
protocol = 1;
publishedid = 1559212036;
name = "Community Framework";
timestamp = 133000000;
"#;

    #[test]
    fn parses_meta_cpp() {
        let m = parse_meta_cpp(META).unwrap();
        assert_eq!(m.workshop_id, 1559212036);
        assert_eq!(m.name, "Community Framework");
    }

    #[test]
    fn scan_reads_installed_dirs() {
        let dir = tempdir().unwrap();
        let mod_dir = dir.path().join("1559212036");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(mod_dir.join("meta.cpp"), META).unwrap();
        let mods = scan_installed_mods(dir.path());
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].workshop_id, 1559212036);
    }

    #[test]
    fn computes_missing_mods() {
        let required = vec![
            ServerMod {
                name: "CF".into(),
                workshop_id: 1,
            },
            ServerMod {
                name: "X".into(),
                workshop_id: 2,
            },
        ];
        let installed = vec![InstalledMod {
            name: "CF".into(),
            workshop_id: 1,
        }];
        assert_eq!(missing_mods(&required, &installed), vec![2]);
    }
}
