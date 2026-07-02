use crate::error::Error;
use crate::process::{self, SteamChannel};
use crate::servers::Server;
use crate::DAYZ_APP_ID;

pub fn build_launch_args(server: &Server, player: &str, password: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-applaunch".into(),
        DAYZ_APP_ID.to_string(),
        "-nolauncher".into(),
        // Equals form, matching `-connect=`/`-port=` below. DayZ ignores the space-separated
        // `-name <value>` form, which silently falls back to the "Survivor" default profile.
        format!("-name={player}"),
    ];
    if !server.mods.is_empty() {
        let mods = server
            .mods
            .iter()
            .map(|m| format!("@{}", m.workshop_id))
            .collect::<Vec<_>>()
            .join(";");
        args.push(format!("-mod={mods}"));
    }
    args.push(format!("-connect={}", server.ip));
    args.push(format!("-port={}", server.game_port));
    if let Some(pw) = password {
        args.push(format!("-password={pw}"));
    }
    args
}

/// Build the `steam://run/<appid>//<args>/` URL that launches DayZ with the given options.
///
/// We hand the launch to Steam as a URL (opened via the desktop portal) instead of spawning
/// `steam -applaunch …` on the host, so the app needs no host-spawn permission. Steam appends the
/// `//<args>` as the game's command-line options. Spaces are percent-encoded (they'd otherwise cut
/// the URL short); the option punctuation Steam parses (`-`, `=`, `;`, `@`, `.`) is left literal.
pub fn build_launch_url(server: &Server, player: &str, password: Option<&str>) -> String {
    let args = build_launch_args(server, player, password).join(" ");
    format!(
        "steam://run/{DAYZ_APP_ID}//{}/",
        args.replace('%', "%25").replace(' ', "%20")
    )
}

/// Hand the launch off to Steam and return immediately (fire-and-forget: there's no child to await).
///
/// The `Direct` and `Pipe` channels deliver the `-applaunch …` **args** (CLI form), which skips the
/// desktop MIME chooser *and* Steam's `steam://run` command-confirmation dialog; the `Portal`
/// fallback opens the `steam://run` **URL** (which may prompt). See [`process::steam_channel`].
pub fn launch(
    channel: &SteamChannel,
    server: &Server,
    player: &str,
    password: Option<&str>,
) -> Result<(), Error> {
    let args = build_launch_args(server, player, password);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let url = build_launch_url(server, player, password);
    log::info!("launching via {channel:?}: steam {}", refs.join(" "));
    process::steam_command_or_uri(channel, &refs, &url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::{Server, ServerMod};

    fn server() -> Server {
        Server {
            name: "S".into(),
            ip: "5.6.7.8".into(),
            game_port: 2302,
            players: 1,
            max_players: 60,
            map: "namalsk".into(),
            time: "12:00".into(),
            first_person: true,
            password: false,
            mods: vec![
                ServerMod {
                    name: "CF".into(),
                    workshop_id: 1,
                },
                ServerMod {
                    name: "NM".into(),
                    workshop_id: 2,
                },
            ],
            version: "1.29.163047".into(),
            version_match: None,
        }
    }

    #[test]
    fn builds_args_with_mods_and_connect() {
        let args = build_launch_args(&server(), "survivor", None);
        assert_eq!(args[0], "-applaunch");
        assert_eq!(args[1], "221100");
        assert!(args.contains(&"-nolauncher".to_string()));
        assert!(args.contains(&"-name=survivor".to_string()));
        assert!(args.contains(&"-mod=@1;@2".to_string()));
        assert!(args.contains(&"-connect=5.6.7.8".to_string()));
        assert!(args.contains(&"-port=2302".to_string()));
    }

    #[test]
    fn includes_password_when_present() {
        let args = build_launch_args(&server(), "survivor", Some("secret"));
        assert!(args.contains(&"-password=secret".to_string()));
    }

    #[test]
    fn omits_mod_arg_when_no_mods() {
        let mut s = server();
        s.mods.clear();
        let args = build_launch_args(&s, "x", None);
        assert!(!args.iter().any(|a| a.starts_with("-mod=")));
    }

    #[test]
    fn builds_run_url_with_encoded_spaces() {
        let url = build_launch_url(&server(), "survivor", None);
        assert!(url.starts_with("steam://run/221100//"));
        assert!(url.ends_with('/'));
        // Spaces between options are encoded; option punctuation stays literal.
        assert!(url.contains("-applaunch%20221100%20-nolauncher"));
        assert!(url.contains("-mod=@1;@2"));
        assert!(url.contains("-connect=5.6.7.8"));
        assert!(!url.contains(' '));
    }
}
