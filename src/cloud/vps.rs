#[derive(Clone, Default)]
pub struct View {
    pub busy: bool,
    pub fingerprint: Option<String>,
    pub previous: Option<String>,
    pub steps: Vec<String>,
    pub error: Option<String>,
    pub complete: bool,
}
#[cfg(wedex_server_bundle)]
pub const BINARY: &[u8] = include_bytes!("../../assets/server/wedex-server-x86_64-linux-musl");
#[cfg(not(wedex_server_bundle))]
pub const BINARY: &[u8] = &[];
