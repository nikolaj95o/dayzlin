pub mod filter;
pub mod model;
pub use filter::{apply_filter, fuzzy_search, ServerFilter};
pub use model::{parse_servers, Server, ServerMod};
