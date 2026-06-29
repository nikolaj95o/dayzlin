use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::servers::Server;

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ServerFilter {
    pub map: Option<String>,
    pub first_person_only: bool,
    pub no_password: bool,
    pub max_mods: Option<usize>,
    pub min_players: Option<u32>,
    pub has_slots: bool,
}

pub fn apply_filter(servers: &[Server], f: &ServerFilter) -> Vec<Server> {
    servers
        .iter()
        .filter(|s| match &f.map {
            Some(m) => s.map.eq_ignore_ascii_case(m),
            None => true,
        })
        .filter(|s| !f.first_person_only || s.first_person)
        .filter(|s| !f.no_password || !s.password)
        .filter(|s| f.max_mods.map_or(true, |max| s.mods.len() <= max))
        .filter(|s| f.min_players.map_or(true, |min| s.players >= min))
        .filter(|s| !f.has_slots || s.players < s.max_players)
        .cloned()
        .collect()
}

pub fn fuzzy_search(servers: &[Server], query: &str) -> Vec<Server> {
    if query.trim().is_empty() {
        return servers.to_vec();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, &Server)> = servers
        .iter()
        .filter_map(|s| matcher.fuzzy_match(&s.name, query).map(|score| (score, s)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, s)| s.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::{Server, ServerMod};

    fn srv(
        name: &str,
        map: &str,
        fp: bool,
        pw: bool,
        players: u32,
        max: u32,
        nmods: usize,
    ) -> Server {
        Server {
            name: name.into(),
            ip: "1.1.1.1".into(),
            game_port: 2302,
            players,
            max_players: max,
            map: map.into(),
            time: "12:00".into(),
            first_person: fp,
            password: pw,
            mods: (0..nmods)
                .map(|i| ServerMod {
                    name: format!("m{i}"),
                    workshop_id: i as u64,
                })
                .collect(),
        }
    }

    #[test]
    fn filters_by_map_and_first_person() {
        let servers = vec![
            srv("A", "namalsk", true, false, 10, 60, 2),
            srv("B", "chernarus", true, false, 5, 60, 2),
            srv("C", "namalsk", false, false, 8, 60, 2),
        ];
        let f = ServerFilter {
            map: Some("namalsk".into()),
            first_person_only: true,
            ..Default::default()
        };
        let out = apply_filter(&servers, &f);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "A");
    }

    #[test]
    fn filters_no_password_and_has_slots() {
        let servers = vec![
            srv("full", "x", true, false, 60, 60, 0),
            srv("open", "x", true, false, 10, 60, 0),
            srv("locked", "x", true, true, 10, 60, 0),
        ];
        let f = ServerFilter {
            no_password: true,
            has_slots: true,
            ..Default::default()
        };
        let out = apply_filter(&servers, &f);
        assert_eq!(
            out.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["open"]
        );
    }

    #[test]
    fn fuzzy_search_ranks_matches_and_drops_nonmatches() {
        let servers = vec![
            srv("Namalsk Hardcore", "namalsk", true, false, 10, 60, 2),
            srv("Chernarus Vanilla", "chernarus", true, false, 10, 60, 0),
        ];
        let out = fuzzy_search(&servers, "nmlsk");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "Namalsk Hardcore");
    }

    #[test]
    fn empty_query_returns_all() {
        let servers = vec![srv("A", "x", true, false, 1, 60, 0)];
        assert_eq!(fuzzy_search(&servers, "").len(), 1);
    }
}
