use std::path::Path;

use crate::error::Error;
use crate::process::CommandRunner;
use crate::servers::ServerMod;
use crate::DAYZ_APP_ID;

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

pub fn ensure_mod_symlinks(game_dir: &Path, workshop_dir: &Path, ids: &[u64]) -> Result<(), Error> {
    for &id in ids {
        let src = workshop_dir.join(id.to_string());
        if !src.is_dir() {
            return Err(Error::ModNotInstalled(id));
        }
        let link = game_dir.join(format!("@{id}"));
        if link.symlink_metadata().is_ok() {
            continue; // already present
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &link)?;
    }
    Ok(())
}

pub async fn download_mod(runner: &dyn CommandRunner, login: &str, id: u64) -> Result<(), Error> {
    if login.is_empty() || login == "anonymous" {
        return Err(Error::AnonymousAccount);
    }
    let app = DAYZ_APP_ID.to_string();
    let id_s = id.to_string();
    let args = [
        "+@ShutdownOnFailedCommand",
        "1",
        "+login",
        login,
        "+workshop_download_item",
        &app,
        &id_s,
        "validate",
        "+quit",
    ];
    log::debug!("running steamcmd workshop_download_item {app} {id_s}");
    let out = runner.run("steamcmd", &args).await?;
    let combined = format!("{}{}", out.stdout, out.stderr);
    if combined.contains("FAILED login") || combined.contains("Invalid Password") {
        log::warn!("steamcmd login failed for mod {id}: {combined}");
        return Err(Error::SteamCmdLogin {
            detail: combined.trim().to_string(),
        });
    }
    if out.status != 0 {
        log::warn!(
            "steamcmd failed for mod {id} (status {}): {combined}",
            out.status
        );
        return Err(Error::CommandFailed {
            program: "steamcmd".into(),
            status: out.status,
            stderr: out.stderr,
        });
    }
    log::debug!("steamcmd downloaded mod {id} successfully");
    Ok(())
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

    use crate::process::{MockRunner, Output};

    #[test]
    fn creates_symlinks_for_installed_mods() {
        let dir = tempdir().unwrap();
        let workshop = dir.path().join("workshop");
        let game = dir.path().join("game");
        fs::create_dir_all(workshop.join("1")).unwrap();
        fs::create_dir_all(&game).unwrap();
        ensure_mod_symlinks(&game, &workshop, &[1]).unwrap();
        let link = game.join("@1");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[test]
    fn symlink_errors_when_workshop_dir_missing() {
        let dir = tempdir().unwrap();
        let workshop = dir.path().join("workshop");
        let game = dir.path().join("game");
        fs::create_dir_all(&workshop).unwrap();
        fs::create_dir_all(&game).unwrap();
        assert!(matches!(
            ensure_mod_symlinks(&game, &workshop, &[99]),
            Err(crate::Error::ModNotInstalled(99))
        ));
    }

    #[tokio::test]
    async fn download_mod_rejects_anonymous() {
        let runner = MockRunner::new();
        let err = download_mod(&runner, "anonymous", 1).await.unwrap_err();
        assert!(matches!(err, crate::Error::AnonymousAccount));
    }

    #[tokio::test]
    async fn download_mod_invokes_steamcmd_with_expected_args() {
        let runner = MockRunner::new().with_response(
            "steamcmd",
            Output {
                status: 0,
                stdout: "Success. Downloaded item".into(),
                stderr: String::new(),
            },
        );
        download_mod(&runner, "user", 1559212036).await.unwrap();
        let (prog, args) = runner.calls()[0].clone();
        assert_eq!(prog, "steamcmd");
        assert!(args.iter().any(|a| a == "+workshop_download_item"));
        assert!(args.iter().any(|a| a == "221100"));
        assert!(args.iter().any(|a| a == "1559212036"));
    }

    #[tokio::test]
    async fn download_mod_maps_login_failure() {
        let runner = MockRunner::new().with_response(
            "steamcmd",
            Output {
                status: 0,
                stdout: "FAILED login with result code Invalid Password".into(),
                stderr: String::new(),
            },
        );
        let err = download_mod(&runner, "user", 1).await.unwrap_err();
        match err {
            crate::Error::SteamCmdLogin { detail } => {
                assert!(detail.contains("Invalid Password"));
            }
            other => panic!("expected SteamCmdLogin, got {other:?}"),
        }
    }
}
