//! Durable collection metadata. Audio and device-local download paths never enter this protocol.
use serde::{Deserialize, Serialize};

pub const PAGE: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub duration: u64,
    pub cover: Option<String>,
    pub page_url: String,
    #[serde(default = "unknown")]
    pub edition: String,
}
fn unknown() -> String {
    "Unknown".into()
}
impl Track {
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.id)
    }
    pub fn valid(&self) -> bool {
        matches!(
            self.provider.as_str(),
            "Yandex" | "Hitmotop" | "SoundCloud" | "YouTube" | "LastFm"
        ) && !self.id.is_empty()
            && self.id.len() <= 512
            && !self.id.chars().any(char::is_control)
            && !self.title.trim().is_empty()
            && self.title.len() <= 1024
            && self.artist.len() <= 1024
            && self.album.len() <= 1024
            && self.duration <= 86400
            && matches!(
                self.edition.as_str(),
                "Unknown" | "Explicit" | "Clean" | "ConfirmedOriginal"
            )
            && self.cover.as_ref().is_none_or(|v| web_url(v, false))
            && web_url(&self.page_url, true)
    }
}
fn web_url(value: &str, empty: bool) -> bool {
    if value.is_empty() {
        return empty;
    }
    value.len() <= 4096
        && url::Url::parse(value).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
        })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub track: Track,
    pub liked: bool,
    pub base: u64,
    pub parent: Option<String>,
    /// Initial union may add missing entries, but must never overwrite a server tombstone.
    #[serde(default)]
    pub seed: bool,
}
impl Operation {
    pub fn valid(&self) -> bool {
        uuid::Uuid::parse_str(&self.id).is_ok()
            && self.track.valid()
            && self.base < i64::MAX as u64
            && self
                .parent
                .as_ref()
                .is_none_or(|v| uuid::Uuid::parse_str(v).is_ok() && v != &self.id)
            && (!self.seed || (self.liked && self.parent.is_none() && self.base == 0))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub track: Track,
    pub liked: bool,
    pub version: u64,
    pub added: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ack {
    pub id: String,
    pub entry: Entry,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delta {
    pub since: u64,
    pub cursor: u64,
    pub more: bool,
    pub entries: Vec<Entry>,
    pub acknowledged: Vec<Ack>,
}
