//! dayz-core: UI-agnostic domain logic for the dayzlin DayZ launcher.

pub mod error;
pub mod process;
pub mod servers;
pub mod steam;

pub use error::Error;

/// Steam App ID for DayZ.
pub const DAYZ_APP_ID: u32 = 221100;
/// dayzsalauncher server list API base URL.
pub const DAYZ_API: &str = "https://dayzsalauncher.com/api/v1";
