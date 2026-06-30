//! Detecting the installed DayZ build version and matching it against a server's reported version.
//!
//! The dayzsalauncher feed reports a server's version as a 3-part string like `1.29.163047`, while
//! the installed game carries a 4-part `1.29.0.163047` in `DayZ_x64.exe`'s PE version resource. We
//! read the latter straight out of the binary (no PE-parsing dependency) and compare the two on
//! `(major, minor, build)`, which collapses both forms to the same key.

use std::path::Path;

/// Extract DayZ's full build (e.g. `1.29.0.163047`) from a Windows PE's embedded version resource.
/// The resource stores keys and values as UTF-16LE strings, so we scan the bytes as UTF-16LE and,
/// once we pass the `ProductVersion` key, return the next value that looks like a dotted numeric
/// version. We deliberately key off `ProductVersion`, not `FileVersion`: on DayZ the latter is just
/// the coarse `1.29` (no build number), which wouldn't match the feed's `1.29.163047`. Hand-rolled
/// to avoid pulling in a PE-parsing crate (cf. the line-scan VDF parsing in `steam.rs`).
pub fn parse_exe_product_version(bytes: &[u8]) -> Option<String> {
    let mut seen_marker = false;
    let mut current = String::new();
    for chunk in bytes.chunks_exact(2) {
        let u = u16::from_le_bytes([chunk[0], chunk[1]]);
        // Printable ASCII extends the current run; anything else (NUL padding, etc.) ends it.
        if (0x20..0x7f).contains(&u) {
            current.push(u as u8 as char);
            continue;
        }
        if let Some(v) = take_token(&mut current, &mut seen_marker) {
            return Some(v);
        }
    }
    take_token(&mut current, &mut seen_marker)
}

/// Consume a completed UTF-16 run: flag the `ProductVersion` marker, or return it if it's the
/// version value that follows one.
fn take_token(current: &mut String, seen_marker: &mut bool) -> Option<String> {
    if current.is_empty() {
        return None;
    }
    let token = std::mem::take(current);
    if token == "ProductVersion" {
        *seen_marker = true;
        None
    } else if *seen_marker && is_version_token(&token) {
        Some(token)
    } else {
        None
    }
}

/// True for a dotted run of numbers with at least a major and minor, e.g. `1.29` or `1.29.0.163047`.
fn is_version_token(s: &str) -> bool {
    let mut parts = 0;
    for seg in s.split('.') {
        if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        parts += 1;
    }
    parts >= 2
}

/// Read the installed DayZ version from `<game_dir>/DayZ_x64.exe`. `None` if the exe is missing or
/// carries no parseable version. The binary is ~18 MB, so callers should memoize the result.
pub fn read_installed_version(game_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(game_dir.join("DayZ_x64.exe")).ok()?;
    parse_exe_product_version(&bytes)
}

/// Whether a server's version matches the installed one. `None` when either side is unknown or
/// unparseable, so callers can treat "can't tell" as "don't flag / don't hide".
pub fn version_match(server_ver: &str, local: Option<&str>) -> Option<bool> {
    let key = version_key(local?)?;
    Some(version_key(server_ver)? == key)
}

/// `(major, minor, build)` where `build` is the last numeric component — collapses the feed's 3-part
/// `1.29.163047` and the exe's 4-part `1.29.0.163047` to the same comparable key.
fn version_key(v: &str) -> Option<(u64, u64, u64)> {
    let nums: Vec<u64> = v
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if nums.len() < 2 {
        return None;
    }
    Some((nums[0], nums[1], *nums.last().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn parses_product_version_after_marker() {
        // Mirror DayZ's real layout: `FileVersion` carries only the coarse `1.29`, and the full
        // build sits under `ProductVersion`. We must skip the former and return the latter.
        let push = |buf: &mut Vec<u8>, s: &str| {
            buf.extend(utf16le(s));
            buf.extend_from_slice(&[0, 0]);
        };
        let mut buf = Vec::new();
        push(&mut buf, "FileVersion");
        push(&mut buf, "1.29");
        push(&mut buf, "ProductName");
        push(&mut buf, "DayZ");
        push(&mut buf, "ProductVersion");
        push(&mut buf, "1.29.0.163047");
        assert_eq!(
            parse_exe_product_version(&buf).as_deref(),
            Some("1.29.0.163047")
        );
    }

    #[test]
    fn ignores_version_without_marker() {
        let buf = utf16le("1.29.0.163047");
        assert_eq!(parse_exe_product_version(&buf), None);
    }

    #[test]
    fn version_match_collapses_three_and_four_part_forms() {
        assert_eq!(version_match("1.29.163047", Some("1.29.0.163047")), Some(true));
        assert_eq!(version_match("1.28.160123", Some("1.29.0.163047")), Some(false));
    }

    #[test]
    fn version_match_unknown_is_none() {
        assert_eq!(version_match("1.29.163047", None), None);
        assert_eq!(version_match("", Some("1.29.0.163047")), None);
        assert_eq!(version_match("bogus", Some("1.29.0.163047")), None);
    }
}
