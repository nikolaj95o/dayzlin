use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Server {
    pub name: String,
    pub ip: String,
    pub game_port: u16,
    pub players: u32,
    pub max_players: u32,
    pub map: String,
    pub time: String,
    pub first_person: bool,
    pub password: bool,
    pub mods: Vec<ServerMod>,
    /// Game build the server runs, as reported by the feed (e.g. `1.29.163047`). `default` keeps
    /// caches written before this field existed loadable.
    #[serde(default)]
    pub version: String,
    /// Whether [`version`] matches the installed DayZ build. Transient: set per request by the
    /// command layer (`None` = unknown / not yet computed), not meaningful in the on-disk cache.
    #[serde(default)]
    pub version_match: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerMod {
    pub name: String,
    pub workshop_id: u64,
}

// ---- raw API shapes (decoupled from our domain model) ----

// The live dayzsalauncher response is `{ "status": 0, "result": [ ...servers... ] }`
// — `result` is the server array directly (confirmed via spike, Task 8 Step 5).
#[derive(Deserialize)]
struct RawResponse {
    result: Vec<RawServer>,
}
#[derive(Deserialize)]
struct RawServer {
    name: String,
    endpoint: RawEndpoint,
    #[serde(rename = "gamePort")]
    game_port: u16,
    players: u32,
    #[serde(rename = "maxPlayers")]
    max_players: u32,
    map: String,
    time: String,
    #[serde(rename = "firstPersonOnly", default)]
    first_person: bool,
    #[serde(default)]
    password: bool,
    #[serde(default)]
    mods: Vec<RawMod>,
    #[serde(default)]
    version: Option<String>,
}
#[derive(Deserialize)]
struct RawEndpoint {
    ip: String,
}
#[derive(Deserialize)]
struct RawMod {
    name: String,
    #[serde(rename = "steamWorkshopId")]
    workshop_id: u64,
}

/// Keep the first server for each `(ip, game_port)`. The endpoint is a unique identity: the UI
/// keys rows by it and favorites/history match on it, and the feed lists some endpoints many
/// times — a duplicate key is a fatal Svelte render error. Applied to both freshly parsed and
/// cached lists so stale duped caches are sanitized too.
pub fn dedupe_by_endpoint(servers: Vec<Server>) -> Vec<Server> {
    let mut seen = std::collections::HashSet::new();
    servers
        .into_iter()
        .filter(|s| seen.insert((s.ip.clone(), s.game_port)))
        .collect()
}

pub fn parse_servers(json: &str) -> Result<Vec<Server>, Error> {
    let raw: RawResponse = serde_json::from_str(json).map_err(|e| Error::Parse(e.to_string()))?;
    let servers = raw
        .result
        .into_iter()
        .map(|r| Server {
            name: r.name,
            ip: r.endpoint.ip,
            game_port: r.game_port,
            players: r.players,
            max_players: r.max_players,
            map: r.map,
            time: r.time,
            first_person: r.first_person,
            password: r.password,
            mods: r
                .mods
                .into_iter()
                .map(|m| ServerMod {
                    name: m.name,
                    workshop_id: m.workshop_id,
                })
                .collect(),
            version: r.version.unwrap_or_default(),
            version_match: None,
        })
        .collect();
    Ok(dedupe_by_endpoint(servers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_fixture() {
        let json = include_str!("../../tests/fixtures/servers.json");
        let servers = parse_servers(json).unwrap();
        assert_eq!(servers.len(), 1);
        let s = &servers[0];
        assert_eq!(s.name, "Test Namalsk PVE");
        assert_eq!(s.ip, "1.2.3.4");
        assert_eq!(s.game_port, 2302);
        assert_eq!(s.version, "1.29.162510");
        assert_eq!(s.first_person, true);
        assert_eq!(s.mods.len(), 2);
        assert_eq!(s.mods[0].workshop_id, 1559212036);
    }

    #[test]
    fn dedupes_repeated_endpoints_keeping_first() {
        // Two entries share 1.2.3.4:2302; a distinct endpoint follows. The feed really does
        // this (e.g. one endpoint listed 18 times) and a duplicate row key crashes the UI.
        let json = r#"{"result":[
            {"name":"First","endpoint":{"ip":"1.2.3.4"},"gamePort":2302,"players":1,"maxPlayers":60,"map":"chernarusplus","time":"01:00"},
            {"name":"Dup","endpoint":{"ip":"1.2.3.4"},"gamePort":2302,"players":2,"maxPlayers":60,"map":"chernarusplus","time":"02:00"},
            {"name":"Other","endpoint":{"ip":"5.6.7.8"},"gamePort":2302,"players":3,"maxPlayers":60,"map":"namalsk","time":"03:00"}
        ]}"#;
        let servers = parse_servers(json).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "First"); // first occurrence wins
        assert_eq!(servers[1].name, "Other");
    }
}
