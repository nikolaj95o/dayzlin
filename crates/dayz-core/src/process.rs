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
        // Kill the child if this future is dropped, so a cancelled probe doesn't leak a process.
        // Under Flatpak the child is the `flatpak-spawn` wrapper, so the host process may not
        // always be reaped.
        cmd.kill_on_drop(true);
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

/// True if the Steam client appears to be running. URL-driven workshop downloads and
/// `steam -applaunch` both need a live, logged-in client, so `run_play` preflights this to fail
/// fast with actionable guidance instead of firing `steam://` URLs into the void. Honors the same
/// sandbox/host-spawn decision as [`RealRunner`], so under Flatpak it probes the *host* process
/// list where Steam actually runs.
pub async fn steam_running(runner: &dyn CommandRunner) -> bool {
    matches!(
        runner.run("pgrep", &["-x", "steamwebhelper"]).await,
        Ok(out) if out.status == 0
    )
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

    #[tokio::test]
    async fn steam_running_reports_present_and_absent() {
        let up = MockRunner::new().with_response(
            "pgrep",
            Output {
                status: 0,
                stdout: "12345".into(),
                stderr: String::new(),
            },
        );
        assert!(steam_running(&up).await);

        let down = MockRunner::new().with_response(
            "pgrep",
            Output {
                status: 1,
                stdout: String::new(),
                stderr: String::new(),
            },
        );
        assert!(!steam_running(&down).await);
    }

    #[tokio::test]
    async fn mock_runner_returns_scripted_output() {
        let runner = MockRunner::new().with_response(
            "pgrep",
            Output {
                status: 0,
                stdout: "Success".into(),
                stderr: String::new(),
            },
        );
        let out = runner.run("pgrep", &["-x", "steam"]).await.unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(out.stdout, "Success");
        assert_eq!(runner.calls()[0].0, "pgrep");
        assert_eq!(runner.calls()[0].1, vec!["-x", "steam"]);
    }
}
