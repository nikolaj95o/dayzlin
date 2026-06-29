use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Clone, serde::Serialize)]
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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerMod {
    pub name: String,
    pub workshop_id: u64,
}

// ---- raw API shapes (decoupled from our domain model) ----

#[derive(Deserialize)]
struct RawResponse {
    result: RawResult,
}
#[derive(Deserialize)]
struct RawResult {
    servers: Vec<RawServer>,
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

pub fn parse_servers(json: &str) -> Result<Vec<Server>, Error> {
    let raw: RawResponse = serde_json::from_str(json).map_err(|e| Error::Parse(e.to_string()))?;
    Ok(raw
        .result
        .servers
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
        })
        .collect())
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
        assert_eq!(s.first_person, true);
        assert_eq!(s.mods.len(), 2);
        assert_eq!(s.mods[0].workshop_id, 1559212036);
    }
}
