pub mod admin;
pub mod client;
mod database;
pub mod mirror;
pub mod protocol;
pub mod server;
pub mod update;
pub mod vault;

pub use protocol::*;
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
