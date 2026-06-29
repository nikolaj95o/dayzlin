use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("steam installation not found")]
    SteamNotFound,
    #[error("path does not exist: {0}")]
    PathMissing(PathBuf),
    #[error("command `{program}` failed (status {status}): {stderr}")]
    CommandFailed {
        program: String,
        status: i32,
        stderr: String,
    },
    #[error("steam account is anonymous; cannot install or update mods")]
    AnonymousAccount,
    #[error("steamcmd login failed; re-login required")]
    SteamCmdLogin { detail: String },
    #[error("mod {0} is not installed")]
    ModNotInstalled(u64),
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
