use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::process::{with_home, CommandRunner};
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

/// Relative path from `link_dir` (the directory that will contain the symlink) to `target`.
/// DayZ runs under Proton; a *relative* symlink target — like dayz-ctl's `ln -sr` — is resolved
/// more reliably by wine than an absolute Linux path. Both paths are absolute in practice (derived
/// from the Steam library root), so a plain component diff yields e.g. `../../workshop/content/...`.
fn relative_link_target(link_dir: &Path, target: &Path) -> PathBuf {
    let from: Vec<Component> = link_dir.components().collect();
    let to: Vec<Component> = target.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a == b)
        .count();
    let mut rel = PathBuf::new();
    for _ in common..from.len() {
        rel.push("..");
    }
    for comp in &to[common..] {
        rel.push(comp.as_os_str());
    }
    rel
}

/// Recursively lowercase every file and directory name under `dir`.
///
/// DayZ runs under Proton on a case-sensitive Linux filesystem and looks for `addons\<x>.pbo`
/// (lowercase), but many workshop mods ship a capital `Addons/`, `Keys/`, or mixed-case `.pbo`
/// names. Wine does not case-fold lookups that traverse the `@<id>` symlink into the raw workshop
/// tree, so those mods are invisible to the engine and the server reports "Missing PBO". This is
/// the standard DayZ-on-Linux remedy. Renames are metadata-only and idempotent (already-lowercase
/// entries are skipped), so it is safe to call on every launch — a `.pbo` and its paired
/// `.pbo.<tag>.bisign` lowercase together, keeping signatures matched.
pub fn lowercase_mod_tree(dir: &Path) -> Result<(), Error> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Recurse first so we rename children before (possibly) renaming the directory itself.
        if path.is_dir() {
            lowercase_mod_tree(&path)?;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower == name {
            continue; // already lowercase
        }
        let dest = dir.join(&lower);
        if dest.symlink_metadata().is_ok() {
            log::warn!(
                "skip lowercasing {}: {} already exists",
                path.display(),
                dest.display()
            );
            continue;
        }
        std::fs::rename(&path, &dest)?;
        log::info!("lowercased {} -> {lower}", path.display());
    }
    Ok(())
}

pub fn ensure_mod_symlinks(game_dir: &Path, workshop_dir: &Path, ids: &[u64]) -> Result<(), Error> {
    for &id in ids {
        let src = workshop_dir.join(id.to_string());
        if !src.is_dir() {
            return Err(Error::ModNotInstalled(id));
        }
        let link = game_dir.join(format!("@{id}"));
        let target = relative_link_target(game_dir, &src);
        // Keep an existing entry only when it's a symlink that already resolves to `src`.
        // A stale/broken/wrong-target link (e.g. from an earlier failed run) is the kind of thing
        // that leaves the game silently missing mods, so repair it; never clobber a real directory.
        if let Ok(meta) = link.symlink_metadata() {
            if !meta.file_type().is_symlink() {
                log::warn!("@{id} exists but is not a symlink; leaving it untouched");
                continue;
            }
            if std::fs::canonicalize(&link).ok() == std::fs::canonicalize(&src).ok() {
                log::info!("mod symlink @{id} already correct -> {}", target.display());
                continue;
            }
            log::info!("repairing stale/broken mod symlink @{id}");
            std::fs::remove_file(&link)?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        log::info!("created mod symlink {} -> {}", link.display(), target.display());
    }
    Ok(())
}

/// Recursively sum the byte sizes of all files under `path`. Returns 0 if `path` is missing or
/// unreadable, so callers can poll it before SteamCMD's staging directory appears.
pub fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => total += dir_size_bytes(&entry.path()),
            Ok(ft) if ft.is_file() => {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
            _ => {}
        }
    }
    total
}

pub async fn download_mod(
    runner: &dyn CommandRunner,
    home: &Path,
    login: &str,
    install_dir: &Path,
    id: u64,
) -> Result<(), Error> {
    if login.is_empty() || login == "anonymous" {
        return Err(Error::AnonymousAccount);
    }
    let app = DAYZ_APP_ID.to_string();
    let id_s = id.to_string();
    let dir = install_dir.to_string_lossy();
    // `+force_install_dir` must precede `+login` for `workshop_download_item` to honor it.
    // Without it SteamCMD writes into its own default tree, which often differs from the
    // Steam client's library, so the downloaded files would never appear where we verify.
    let args = [
        "+@ShutdownOnFailedCommand",
        "1",
        "+force_install_dir",
        &dir,
        "+login",
        login,
        "+workshop_download_item",
        &app,
        &id_s,
        "validate",
        "+quit",
    ];
    // Run under an isolated HOME so SteamCMD's bootstrap can't rewrite the Steam client's shared
    // `libraryfolders.vdf` and drop the DayZ library (see `process::with_home`). `+force_install_dir`
    // is an explicit absolute path, so content still lands in the same library tree we verify below.
    let (prog, full) = with_home(home, "steamcmd", &args);
    let full_refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    log::debug!("running steamcmd workshop_download_item {app} {id_s}");
    let out = runner.run(&prog, &full_refs).await?;
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
    fn dir_size_sums_files_recursively_and_missing_is_zero() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.bin"), vec![0u8; 250]).unwrap();
        assert_eq!(dir_size_bytes(dir.path()), 350);
        assert_eq!(dir_size_bytes(&dir.path().join("nope")), 0);
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
        // Relative target (like dayz-ctl's `ln -sr`) that still resolves to the workshop dir.
        assert_eq!(fs::read_link(&link).unwrap(), PathBuf::from("../workshop/1"));
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(workshop.join("1")).unwrap()
        );
    }

    #[test]
    fn repairs_stale_broken_symlink() {
        let dir = tempdir().unwrap();
        let workshop = dir.path().join("workshop");
        let game = dir.path().join("game");
        fs::create_dir_all(workshop.join("1")).unwrap();
        fs::create_dir_all(&game).unwrap();
        // A pre-existing link pointing at a now-missing target: previously this was skipped,
        // leaving the game without the mod.
        let link = game.join("@1");
        std::os::unix::fs::symlink("/nonexistent/old/target", &link).unwrap();
        ensure_mod_symlinks(&game, &workshop, &[1]).unwrap();
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(workshop.join("1")).unwrap()
        );
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

    #[test]
    fn lowercases_capital_addons_tree() {
        let dir = tempdir().unwrap();
        let m = dir.path().join("1827241477");
        fs::create_dir_all(m.join("Addons")).unwrap();
        fs::create_dir_all(m.join("Keys")).unwrap();
        fs::write(m.join("Addons/HDSN_BreachingCharge.PBO"), b"x").unwrap();
        fs::write(m.join("Addons/HDSN_BreachingCharge.PBO.HDSN.bisign"), b"x").unwrap();
        fs::write(m.join("Keys/HDSN.bikey"), b"x").unwrap();

        lowercase_mod_tree(&m).unwrap();

        assert!(m.join("addons/hdsn_breachingcharge.pbo").is_file());
        assert!(m.join("addons/hdsn_breachingcharge.pbo.hdsn.bisign").is_file());
        assert!(m.join("keys/hdsn.bikey").is_file());
        assert!(!m.join("Addons").exists());
        assert!(!m.join("Keys").exists());
    }

    #[test]
    fn lowercase_is_idempotent_noop_on_lowercase_tree() {
        let dir = tempdir().unwrap();
        let m = dir.path().join("1559212036");
        fs::create_dir_all(m.join("addons")).unwrap();
        fs::write(m.join("addons/assets.pbo"), b"x").unwrap();
        fs::write(m.join("meta.cpp"), b"x").unwrap();

        lowercase_mod_tree(&m).unwrap();
        lowercase_mod_tree(&m).unwrap(); // second pass changes nothing

        assert!(m.join("addons/assets.pbo").is_file());
        assert!(m.join("meta.cpp").is_file());
    }

    #[test]
    fn lowercase_skips_on_collision() {
        let dir = tempdir().unwrap();
        let m = dir.path().join("mod");
        // Both casings present (pathological): the capital one is left untouched, not clobbered.
        fs::create_dir_all(m.join("addons")).unwrap();
        fs::write(m.join("addons/keep.pbo"), b"keep").unwrap();
        fs::create_dir_all(m.join("Addons")).unwrap();
        fs::write(m.join("Addons/other.pbo"), b"other").unwrap();

        lowercase_mod_tree(&m).unwrap();

        assert!(m.join("Addons").exists()); // collision: left in place
        assert_eq!(fs::read(m.join("addons/keep.pbo")).unwrap(), b"keep");
    }

    #[tokio::test]
    async fn download_mod_rejects_anonymous() {
        let runner = MockRunner::new();
        let err = download_mod(&runner, Path::new("/h"), "anonymous", Path::new("/steam"), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::Error::AnonymousAccount));
    }

    #[tokio::test]
    async fn download_mod_invokes_steamcmd_with_expected_args() {
        // Runs via an isolated HOME, so the spawned program is `env HOME=… steamcmd …`.
        let runner = MockRunner::new().with_response(
            "env",
            Output {
                status: 0,
                stdout: "Success. Downloaded item".into(),
                stderr: String::new(),
            },
        );
        download_mod(
            &runner,
            Path::new("/home/me/.local/share/dayzlin/steamcmd-home"),
            "user",
            Path::new("/home/me/.steam/steam"),
            1559212036,
        )
        .await
        .unwrap();
        let (prog, args) = runner.calls()[0].clone();
        assert_eq!(prog, "env");
        assert!(args.iter().any(|a| a == "HOME=/home/me/.local/share/dayzlin/steamcmd-home"));
        assert!(args.iter().any(|a| a == "steamcmd"));
        assert!(args.iter().any(|a| a == "+force_install_dir"));
        assert!(args.iter().any(|a| a == "/home/me/.steam/steam"));
        assert!(args.iter().any(|a| a == "+workshop_download_item"));
        assert!(args.iter().any(|a| a == "221100"));
        assert!(args.iter().any(|a| a == "1559212036"));
    }

    #[tokio::test]
    async fn download_mod_maps_login_failure() {
        let runner = MockRunner::new().with_response(
            "env",
            Output {
                status: 0,
                stdout: "FAILED login with result code Invalid Password".into(),
                stderr: String::new(),
            },
        );
        let err = download_mod(&runner, Path::new("/h"), "user", Path::new("/steam"), 1)
            .await
            .unwrap_err();
        match err {
            crate::Error::SteamCmdLogin { detail } => {
                assert!(detail.contains("Invalid Password"));
            }
            other => panic!("expected SteamCmdLogin, got {other:?}"),
        }
    }
}
