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
        self.calls
            .lock()
            .unwrap()
            .push((program.to_string(), args.iter().map(|s| s.to_string()).collect()));
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

#[cfg(test)]
mod tests {
    use super::*;

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
