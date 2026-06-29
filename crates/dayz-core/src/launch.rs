use crate::error::Error;
use crate::process::CommandRunner;
use crate::servers::Server;
use crate::DAYZ_APP_ID;

pub fn build_launch_args(server: &Server, player: &str, password: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-applaunch".into(),
        DAYZ_APP_ID.to_string(),
        "-nolauncher".into(),
        "-name".into(),
        player.into(),
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

pub async fn launch(runner: &dyn CommandRunner, args: &[String]) -> Result<(), Error> {
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    runner.run("steam", &arg_refs).await?;
    Ok(())
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
        }
    }

    #[test]
    fn builds_args_with_mods_and_connect() {
        let args = build_launch_args(&server(), "survivor", None);
        assert_eq!(args[0], "-applaunch");
        assert_eq!(args[1], "221100");
        assert!(args.contains(&"-nolauncher".to_string()));
        assert!(args.contains(&"-name".to_string()));
        assert!(args.contains(&"survivor".to_string()));
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
}
