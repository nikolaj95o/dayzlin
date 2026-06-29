use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[&str]) -> Result<Output, Error>;
}

/// True iff dayzlin itself is running inside a Flatpak sandbox.
pub fn is_sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}

pub struct RealRunner {
    sandboxed: bool,
}

impl RealRunner {
    pub fn new() -> Self {
        Self {
            sandboxed: is_sandboxed(),
        }
    }
}

impl Default for RealRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for RealRunner {
    async fn run(&self, program: &str, args: &[&str]) -> Result<Output, Error> {
        let mut cmd = if self.sandboxed {
            let mut c = tokio::process::Command::new("flatpak-spawn");
            c.arg("--host").arg(program).args(args);
            c
        } else {
            let mut c = tokio::process::Command::new(program);
            c.args(args);
            c
        };
        let out = cmd.output().await?;
        Ok(Output {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Test/double runner: returns scripted output keyed by program name and records calls.
pub struct MockRunner {
    responses: Mutex<Vec<(String, Output)>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl MockRunner {
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn with_response(self, program: &str, out: Output) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push((program.to_string(), out));
        self
    }

    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for MockRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandRunner for MockRunner {
    async fn run(&self, program: &str, args: &[&str]) -> Result<Output, Error> {
        self.calls.lock().unwrap().push((
            program.to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        ));
        let responses = self.responses.lock().unwrap();
        responses
            .iter()
            .find(|(p, _)| p == program)
            .map(|(_, o)| o.clone())
            .ok_or_else(|| Error::CommandFailed {
                program: program.to_string(),
                status: -1,
                stderr: "no mock response".into(),
            })
    }
}

/// Common Linux terminal emulators, in rough order of preference.
pub const DEFAULT_TERMINALS: &[&str] = &[
    "konsole",
    "gnome-terminal",
    "xfce4-terminal",
    "alacritty",
    "kitty",
    "foot",
    "xterm",
];

/// Return the first `candidates` program found on `PATH`, if any.
pub fn detect_terminal(candidates: &[&str]) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for cand in candidates {
        for dir in std::env::split_paths(&path) {
            if dir.join(cand).is_file() {
                return Some((*cand).to_string());
            }
        }
    }
    None
}

/// steamcmd arguments for a one-time interactive login: `+login <user> +quit`.
pub fn steamcmd_login_argv(user: &str) -> Vec<String> {
    vec!["+login".into(), user.into(), "+quit".into()]
}

/// Build `(program, args)` that opens `term` running an interactive steamcmd login.
/// gnome-terminal needs `--` before the command; most others use `-e`.
pub fn terminal_login_command(term: &str, user: &str) -> (String, Vec<String>) {
    let sep = if term == "gnome-terminal" { "--" } else { "-e" };
    let mut args = vec![sep.to_string(), "steamcmd".to_string()];
    args.extend(steamcmd_login_argv(user));
    (term.to_string(), args)
}

/// Spawn `program args` interactively (output not captured, not awaited).
/// Honors the same sandbox/host-spawn decision as [`RealRunner`].
pub fn spawn_detached(program: &str, args: &[String]) -> Result<(), Error> {
    let (prog, full_args) = if is_sandboxed() {
        let mut v = vec!["--host".to_string(), program.to_string()];
        v.extend(args.iter().cloned());
        ("flatpak-spawn".to_string(), v)
    } else {
        (program.to_string(), args.to_vec())
    };
    std::process::Command::new(&prog)
        .args(&full_args)
        .spawn()
        .map(|_| ())
        .map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steamcmd_login_argv_builds_login_quit() {
        assert_eq!(
            steamcmd_login_argv("alice"),
            vec!["+login", "alice", "+quit"]
        );
    }

    #[test]
    fn terminal_login_command_uses_dash_e_for_konsole() {
        let (prog, args) = terminal_login_command("konsole", "alice");
        assert_eq!(prog, "konsole");
        assert_eq!(args, vec!["-e", "steamcmd", "+login", "alice", "+quit"]);
    }

    #[test]
    fn terminal_login_command_uses_double_dash_for_gnome_terminal() {
        let (_prog, args) = terminal_login_command("gnome-terminal", "bob");
        assert_eq!(args[0], "--");
        assert_eq!(args[1], "steamcmd");
    }

    #[test]
    fn detect_terminal_finds_present_and_skips_absent() {
        // `sh` exists on any POSIX PATH; the bogus one does not.
        assert_eq!(
            detect_terminal(&["definitely-not-a-real-term-xyz", "sh"]),
            Some("sh".into())
        );
        assert_eq!(detect_terminal(&["definitely-not-a-real-term-xyz"]), None);
    }

    #[tokio::test]
    async fn mock_runner_returns_scripted_output() {
        let runner = MockRunner::new().with_response(
            "steamcmd",
            Output {
                status: 0,
                stdout: "Success".into(),
                stderr: String::new(),
            },
        );
        let out = runner.run("steamcmd", &["+quit"]).await.unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(out.stdout, "Success");
        assert_eq!(runner.calls()[0].0, "steamcmd");
        assert_eq!(runner.calls()[0].1, vec!["+quit"]);
    }
}
