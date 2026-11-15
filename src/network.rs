//! Native connectivity, without probing a third-party host or blocking rendering.
use std::sync::atomic::{AtomicU8, Ordering};
static NETWORK: AtomicU8 = AtomicU8::new(2);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    Offline,
    Limited,
    #[default]
    Online,
}
pub fn state() -> State {
    match NETWORK.load(Ordering::Relaxed) {
        0 => State::Offline,
        1 => State::Limited,
        _ => State::Online,
    }
}
#[cfg(target_os = "android")]
pub fn set(value: u8) {
    NETWORK.store(value.min(2), Ordering::Relaxed);
}
