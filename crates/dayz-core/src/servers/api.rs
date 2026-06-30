use std::collections::HashMap;

use crate::error::Error;
use crate::servers::{parse_servers, Server};
use crate::DAYZ_API;

pub async fn fetch_servers(client: &reqwest::Client) -> Result<Vec<Server>, Error> {
    let url = format!("{DAYZ_API}/launcher/servers/dayz");
    let resp = client
        .get(&url)
        .header("User-Agent", "dayzlin")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    parse_servers(&text)
}

const STEAM_FILE_DETAILS: &str =
    "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";

/// Best-effort total download sizes (bytes) for workshop items, keyed by id.
/// Uses Steam's public GetPublishedFileDetails endpoint (no API key required).
/// Returns only the ids Steam reported a size for; callers treat absence as
/// "unknown" and fall back to a totals-less progress display. Any network or
/// parse failure yields an empty map rather than an error, so a launch is never
/// blocked just because size lookup failed.
pub async fn fetch_mod_sizes(client: &reqwest::Client, ids: &[u64]) -> HashMap<u64, u64> {
    if ids.is_empty() {
        return HashMap::new();
    }
    // GetPublishedFileDetails takes a form-encoded list: itemcount + publishedfileids[i].
    let mut form: Vec<(String, String)> = vec![("itemcount".into(), ids.len().to_string())];
    for (i, id) in ids.iter().enumerate() {
        form.push((format!("publishedfileids[{i}]"), id.to_string()));
    }
    let text = match client
        .post(STEAM_FILE_DETAILS)
        .header("User-Agent", "dayzlin")
        .form(&form)
        .send()
        .await
    {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(e) => {
                log::warn!("mod size lookup: reading body failed: {e}");
                return HashMap::new();
            }
        },
        Err(e) => {
            log::warn!("mod size lookup request failed: {e}");
            return HashMap::new();
        }
    };
    parse_mod_sizes(&text)
}

/// Parse an `id -> file_size` map out of a GetPublishedFileDetails JSON body.
/// Steam returns `publishedfileid` and `file_size` as JSON *strings*; entries
/// that are missing a size, zero, or unparseable are skipped.
fn parse_mod_sizes(text: &str) -> HashMap<u64, u64> {
    let mut out = HashMap::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        log::warn!("mod size lookup: response was not valid JSON");
        return out;
    };
    let Some(details) = json
        .get("response")
        .and_then(|r| r.get("publishedfiledetails"))
        .and_then(|d| d.as_array())
    else {
        return out;
    };
    for item in details {
        let id = item
            .get("publishedfileid")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());
        // file_size is a string in this API, but tolerate a numeric form too.
        let size = item.get("file_size").and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| v.as_u64())
        });
        if let (Some(id), Some(size)) = (id, size) {
            if size > 0 {
                out.insert(id, size);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_mod_sizes;

    #[test]
    fn parses_sizes_and_skips_bad_entries() {
        // Real-shaped GetPublishedFileDetails body: string fields, plus a zero-size
        // and a missing-size entry that must be skipped.
        let body = r#"{
            "response": {
                "result": 1,
                "resultcount": 4,
                "publishedfiledetails": [
                    { "publishedfileid": "1559212036", "result": 1, "file_size": "262144000" },
                    { "publishedfileid": "1234567890", "result": 1, "file_size": "0" },
                    { "publishedfileid": "5555555555", "result": 9 },
                    { "publishedfileid": "1827241477", "result": 1, "file_size": "1048576" }
                ]
            }
        }"#;
        let sizes = parse_mod_sizes(body);
        assert_eq!(sizes.get(&1559212036), Some(&262144000));
        assert_eq!(sizes.get(&1827241477), Some(&1048576));
        assert!(!sizes.contains_key(&1234567890)); // zero size skipped
        assert!(!sizes.contains_key(&5555555555)); // missing file_size skipped
        assert_eq!(sizes.len(), 2);
    }

    #[test]
    fn invalid_json_yields_empty_map() {
        assert!(parse_mod_sizes("not json").is_empty());
        assert!(parse_mod_sizes("{}").is_empty());
    }
}
