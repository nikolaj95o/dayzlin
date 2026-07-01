use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;
use crate::servers::ServerMod;
use crate::steam::DayzInstall;
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
/// unreadable, so callers can poll it before Steam's staging directory appears.
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

/// `steam://` URL that asks a running, logged-in Steam client to download a workshop item
/// *without* subscribing to it. Opening the item's `CommunityFilePage` with a chained
/// `+workshop_download_item <appid> <id>` console command triggers a one-shot download into the
/// workshop content tree — the same mechanism dztui uses. This keeps the user's Steam Workshop
/// subscriptions untouched (matching dayzlin's "fetch only what this server needs" model), at the
/// cost of Steam not auto-updating the item later (we own staleness).
pub fn workshop_download_url(id: u64) -> String {
    format!("steam://url/CommunityFilePage/{id}+workshop_download_item {DAYZ_APP_ID} {id}")
}

/// True once a finished workshop item is present in the content tree. Steam writes the item's
/// `meta.cpp` only after the download fully lands (in-progress data lives under
/// `workshop/downloads/…`), so its presence is a reliable "done" signal.
pub fn is_download_complete(workshop_dir: &Path, id: u64) -> bool {
    workshop_dir.join(id.to_string()).join("meta.cpp").exists()
}

/// Undo a `+workshop_download_item` request that was cancelled or never finished. Because that
/// command downloads *without* subscribing (see [`workshop_download_url`]), Steam leaves no
/// subscription to remove — instead the pending download lives on as partial staging data plus a
/// `NeedsDownload`/`WorkshopItemDetails` entry in `appworkshop_<appid>.acf`, and Steam silently
/// resumes it on the next start. This removes both so the item is truly forgotten.
///
/// **Only safe with the Steam client closed** — Steam rewrites this manifest from memory while
/// running and would clobber the edit (or re-stage the deleted data). Callers gate on
/// [`crate::process::steam_running`]. A *completed* download is never touched: its content is a real
/// install we link into the game.
pub fn remove_workshop_download(dayz: &DayzInstall, id: u64) -> Result<(), Error> {
    if is_download_complete(&dayz.workshop_dir(), id) {
        return Ok(());
    }
    let staging = dayz.workshop_downloads_dir().join(id.to_string());
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let manifest = dayz.workshop_manifest_path();
    if let Ok(body) = std::fs::read_to_string(&manifest) {
        let updated = strip_workshop_item(&body, id);
        if updated != body {
            std::fs::write(&manifest, updated)?;
        }
    }
    Ok(())
}

/// Every `"..."` token on a VDF line, in order. A key/value line yields two; a block-opener key
/// (e.g. `"WorkshopItemDetails"` or an item id, which is followed by its own `{`) yields one.
fn quoted_tokens(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            out.push(line[start..j].to_string());
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Numeric item ids listed under `WorkshopItemsInstalled` and under `WorkshopItemDetails`, walking
/// the VDF by brace depth (no full parser, matching the line-scan style used elsewhere here).
fn workshop_section_ids(acf: &str) -> (HashSet<u64>, HashSet<u64>) {
    let (mut installed, mut details) = (HashSet::new(), HashSet::new());
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for line in acf.lines() {
        let t = line.trim();
        if t == "{" {
            let key = pending.take().unwrap_or_default();
            if let (Some(section), Ok(item)) = (stack.last(), key.parse::<u64>()) {
                match section.as_str() {
                    "WorkshopItemsInstalled" => {
                        installed.insert(item);
                    }
                    "WorkshopItemDetails" => {
                        details.insert(item);
                    }
                    _ => {}
                }
            }
            stack.push(key);
        } else if t == "}" {
            stack.pop();
        } else {
            let toks = quoted_tokens(t);
            pending = (toks.len() == 1).then(|| toks[0].clone());
        }
    }
    (installed, details)
}

/// Drop item `id`'s block from the `WorkshopItemDetails` section of an `appworkshop_*.acf` body and,
/// when no pending (listed-but-not-installed) items remain afterward, clear the top-level
/// `NeedsDownload` flag. `WorkshopItemsInstalled` and every other item are left byte-for-byte intact.
fn strip_workshop_item(acf: &str, id: u64) -> String {
    let id_str = id.to_string();
    let (installed, details) = workshop_section_ids(acf);
    let clear_needs_download = details
        .iter()
        .filter(|d| **d != id)
        .all(|d| installed.contains(d));

    let mut out: Vec<String> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut pending_key: Option<String> = None;
    // A block-opener key line is held until its `{` arrives, so we can drop both together when the
    // block is the one being removed.
    let mut pending_line: Option<String> = None;
    let mut skip_depth: Option<usize> = None;

    for line in acf.lines() {
        let t = line.trim();
        if t == "{" {
            let key = pending_key.take().unwrap_or_default();
            let held = pending_line.take();
            let entering_target = skip_depth.is_none()
                && stack.last().map(|s| s == "WorkshopItemDetails").unwrap_or(false)
                && key == id_str;
            stack.push(key);
            if entering_target {
                skip_depth = Some(stack.len());
                continue; // drop the held opener key line and this `{`
            }
            if skip_depth.is_none() {
                if let Some(h) = held {
                    out.push(h);
                }
                out.push(line.to_string());
            }
            continue;
        }
        if t == "}" {
            let depth = stack.len();
            stack.pop();
            if skip_depth == Some(depth) {
                skip_depth = None;
                continue; // drop the target block's closing brace
            }
            if skip_depth.is_none() {
                out.push(line.to_string());
            }
            continue;
        }
        // Defensive: flush a held opener that wasn't followed by `{` (not expected in valid VDF).
        if let Some(h) = pending_line.take() {
            if skip_depth.is_none() {
                out.push(h);
            }
        }
        let toks = quoted_tokens(t);
        if toks.len() == 1 {
            pending_key = Some(toks[0].clone());
            pending_line = Some(line.to_string());
            continue;
        }
        pending_key = None;
        if skip_depth.is_some() {
            continue;
        }
        let at_top = stack.last().map(|s| s == "AppWorkshop").unwrap_or(false);
        if clear_needs_download && at_top && toks.first().map(|k| k == "NeedsDownload") == Some(true)
        {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            out.push(format!("{indent}\"NeedsDownload\"\t\t\"0\""));
        } else {
            out.push(line.to_string());
        }
    }

    let mut result = out.join("\n");
    if acf.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::ServerMod;
    use crate::steam::DayzInstall;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::tempdir;

    /// A minimal `appworkshop_221100.acf`: items 1 & 2 fully installed, item 3 pending
    /// (listed in details only), `NeedsDownload` set.
    const WORKSHOP_ACF: &str = "\"AppWorkshop\"
{
\t\"appid\"\t\t\"221100\"
\t\"NeedsDownload\"\t\t\"1\"
\t\"WorkshopItemsInstalled\"
\t{
\t\t\"1\"
\t\t{
\t\t\t\"size\"\t\t\"100\"
\t\t}
\t\t\"2\"
\t\t{
\t\t\t\"size\"\t\t\"200\"
\t\t}
\t}
\t\"WorkshopItemDetails\"
\t{
\t\t\"1\"
\t\t{
\t\t\t\"manifest\"\t\t\"111\"
\t\t}
\t\t\"2\"
\t\t{
\t\t\t\"manifest\"\t\t\"222\"
\t\t}
\t\t\"3\"
\t\t{
\t\t\t\"manifest\"\t\t\"333\"
\t\t}
\t}
}
";

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

    #[test]
    fn workshop_download_url_targets_community_file_page_with_download_command() {
        assert_eq!(
            workshop_download_url(1559212036),
            "steam://url/CommunityFilePage/1559212036+workshop_download_item 221100 1559212036"
        );
    }

    #[test]
    fn is_download_complete_tracks_meta_cpp() {
        let dir = tempdir().unwrap();
        assert!(!is_download_complete(dir.path(), 42));
        let m = dir.path().join("42");
        fs::create_dir_all(&m).unwrap();
        assert!(!is_download_complete(dir.path(), 42)); // dir exists, meta.cpp not yet
        fs::write(m.join("meta.cpp"), META).unwrap();
        assert!(is_download_complete(dir.path(), 42));
    }

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

    #[test]
    fn strip_removes_pending_item_and_clears_needs_download() {
        // Item 3 is the only pending one; removing it leaves every listed detail installed.
        let out = strip_workshop_item(WORKSHOP_ACF, 3);
        let (installed, details) = workshop_section_ids(&out);
        assert_eq!(installed, HashSet::from([1, 2])); // installs untouched
        assert_eq!(details, HashSet::from([1, 2])); // detail 3 gone
        assert!(out.contains("\"NeedsDownload\"\t\t\"0\""));
        assert!(!out.contains("\"333\"")); // the removed block's contents are gone
    }

    #[test]
    fn strip_keeps_needs_download_when_a_pending_item_remains() {
        // Remove detail 1 (installed) while pending item 3 is still listed: nothing to clear.
        let out = strip_workshop_item(WORKSHOP_ACF, 1);
        let (installed, details) = workshop_section_ids(&out);
        assert!(installed.contains(&1)); // WorkshopItemsInstalled entry for 1 is left in place
        assert_eq!(details, HashSet::from([2, 3])); // only the detail entry for 1 is removed
        assert!(out.contains("\"NeedsDownload\"\t\t\"1\""));
    }

    fn seed_manifest(dayz: &DayzInstall, body: &str) {
        let manifest = dayz.workshop_manifest_path();
        fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        fs::write(&manifest, body).unwrap();
    }

    fn seed_staging(dayz: &DayzInstall, id: u64) {
        let staging = dayz.workshop_downloads_dir().join(id.to_string());
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("part.bin"), vec![0u8; 64]).unwrap();
    }

    #[test]
    fn remove_download_deletes_staging_and_strips_manifest() {
        let dir = tempdir().unwrap();
        let dayz = DayzInstall {
            root: dir.path().to_path_buf(),
        };
        seed_manifest(&dayz, WORKSHOP_ACF);
        seed_staging(&dayz, 3);

        remove_workshop_download(&dayz, 3).unwrap();

        assert!(!dayz.workshop_downloads_dir().join("3").exists());
        let body = fs::read_to_string(dayz.workshop_manifest_path()).unwrap();
        let (_, details) = workshop_section_ids(&body);
        assert!(!details.contains(&3));
        assert!(body.contains("\"NeedsDownload\"\t\t\"0\""));
    }

    #[test]
    fn remove_download_never_touches_a_completed_item() {
        let dir = tempdir().unwrap();
        let dayz = DayzInstall {
            root: dir.path().to_path_buf(),
        };
        // A finished install: content/<id>/meta.cpp exists.
        let content = dayz.workshop_dir().join("3");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("meta.cpp"), META).unwrap();
        seed_manifest(&dayz, WORKSHOP_ACF);
        seed_staging(&dayz, 3);

        remove_workshop_download(&dayz, 3).unwrap();

        // Everything left as-is because the item is a real, complete install.
        assert!(dayz.workshop_downloads_dir().join("3").exists());
        let body = fs::read_to_string(dayz.workshop_manifest_path()).unwrap();
        assert_eq!(body, WORKSHOP_ACF);
    }
}
