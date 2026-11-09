use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub page_url: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub liked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Playback {
    pub track: Option<Track>,
    pub queue: Vec<Track>,
    pub cursor: Option<usize>,
    pub playing: bool,
    pub loading: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub repeat: bool,
    pub shuffle: bool,
    #[serde(default)]
    pub eq: [f32; 10],
    #[serde(default)]
    pub preamp: f32,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            track: None,
            queue: vec![],
            cursor: None,
            playing: false,
            loading: false,
            position: 0.,
            duration: 0.,
            volume: 0.65,
            repeat: false,
            shuffle: false,
            eq: [0.; 10],
            preamp: 0.,
        }
    }
}
impl Playback {
    pub fn valid(&self) -> bool {
        self.queue.len() <= 500
            && self.track.iter().chain(self.queue.iter()).all(|t| {
                t.key.len() <= 1024
                    && t.title.len() <= 1024
                    && t.artist.len() <= 1024
                    && t.duration.is_finite()
                    && (0. ..=86400.).contains(&t.duration)
                    && t.cover.as_ref().is_none_or(|s| {
                        s.len() <= 4096 && (s.starts_with("https://") || s.starts_with("http://"))
                    })
                    && t.page_url.len() <= 4096
                    && t.album.len() <= 1024
            })
            && self.cursor.is_none_or(|i| i < self.queue.len())
            && self.position.is_finite()
            && (0. ..=86400.).contains(&self.position)
            && self.duration.is_finite()
            && (0. ..=86400.).contains(&self.duration)
            && self.volume.is_finite()
            && (0. ..=1.).contains(&self.volume)
            && self
                .eq
                .iter()
                .all(|g| g.is_finite() && (-12. ..=12.).contains(g))
            && self.preamp.is_finite()
            && (-12. ..=12.).contains(&self.preamp)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SetPlaying { playing: bool },
    Next,
    Previous,
    Seek { seconds: f64 },
    Volume { value: f32 },
    Repeat { enabled: bool },
    Shuffle { enabled: bool },
    QueueIndex { index: usize },
    Like { key: String, liked: bool },
    Equalizer { gains: [f32; 10], preamp: f32 },
    PlayTrack { track: Box<Track>, radio: bool },
    PlayCollection { tracks: Vec<Track>, index: usize },
}
impl Action {
    pub fn valid(&self) -> bool {
        match self {
            Self::Seek { seconds } => seconds.is_finite() && (0. ..=86400.).contains(seconds),
            Self::Volume { value } => value.is_finite() && (0. ..=1.).contains(value),
            Self::QueueIndex { index } => *index < 500,
            Self::Like { key, .. } => !key.is_empty() && key.len() <= 1024,
            Self::Equalizer { gains, preamp } => {
                gains
                    .iter()
                    .all(|g| g.is_finite() && (-12. ..=12.).contains(g))
                    && preamp.is_finite()
                    && (-12. ..=12.).contains(preamp)
            }
            Self::PlayTrack { track, .. } => Playback {
                track: Some((**track).clone()),
                ..Default::default()
            }
            .valid(),
            Self::PlayCollection { tracks, index } => {
                !tracks.is_empty()
                    && *index < tracks.len()
                    && Playback {
                        queue: tracks.clone(),
                        ..Default::default()
                    }
                    .valid()
            }
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteCommand {
    pub id: String,
    pub target: String,
    pub action: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delivery {
    pub command: RemoteCommand,
    pub expires_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub can_play: bool,
    pub online: bool,
    pub updated_ms: u64,
    pub playback: Playback,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub revision: u64,
    pub track: Track,
    pub started_ms: u64,
    pub listened_ms: u64,
}
impl Session {
    pub fn valid(&self) -> bool {
        uuid::Uuid::parse_str(&self.id).is_ok()
            && self.revision > 0
            && self.revision < i64::MAX as u64
            && self.listened_ms <= 86_400_000
            && self.started_ms > 0
            && self.started_ms <= crate::now_ms() + 300_000
            && !self.track.key.is_empty()
            && self.track.key.len() <= 1024
            && self.track.title.len() <= 1024
            && self.track.artist.len() <= 1024
            && self.track.duration.is_finite()
            && (0. ..=86400.).contains(&self.track.duration)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Statistics {
    pub listened_ms: u64,
    pub sessions: u64,
    pub top_tracks: Vec<TopTrack>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopTrack {
    pub title: String,
    pub artist: String,
    pub listened_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairRequest {
    pub code: String,
    pub name: String,
    pub can_play: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub device_id: String,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invitation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_key: Option<String>,
    pub endpoint: String,
    pub certificate: String,
    pub code: String,
}
impl Invitation {
    pub fn encode(&self) -> String {
        use base64::Engine;
        format!(
            "wedex-cloud:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(self).expect("Invitation serializes"))
        )
    }
    pub fn decode(value: &str) -> anyhow::Result<Self> {
        use base64::Engine;
        anyhow::ensure!(value.len() <= 32768, "Приглашение слишком длинное");
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(
            value
                .trim()
                .strip_prefix("wedex-cloud:")
                .ok_or_else(|| anyhow::anyhow!("Нужен код приглашения WeDex"))?,
        )?;
        let invite: Self = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            invite
                .sync_key
                .as_ref()
                .is_none_or(|k| crate::vault::valid_key(k)),
            "Некорректный ключ приглашения"
        );
        let url = reqwest::Url::parse(&invite.endpoint)?;
        anyhow::ensure!(
            url.scheme() == "https"
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "Некорректный адрес сервера"
        );
        anyhow::ensure!(
            invite.code.len() <= 128 && !invite.certificate.is_empty(),
            "Неполное приглашение"
        );
        Ok(invite)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    VaultGet,
    VaultSet { entries: Vec<crate::vault::Entry> },
    Authenticate { token: String },
    Publish { playback: Playback },
    Command { command: RemoteCommand },
    Acknowledge { id: String, error: Option<String> },
    Sessions { sessions: Vec<Session> },
    Ping,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    VaultSnapshot {
        entries: Vec<crate::vault::Entry>,
    },
    Welcome {
        #[serde(default)]
        version: String,
        #[serde(default)]
        capabilities: Vec<String>,
        device_id: String,
        api_version: u32,
    },
    Snapshot {
        devices: Vec<Device>,
        statistics: Statistics,
    },
    Execute {
        delivery: Delivery,
    },
    CommandStatus {
        id: String,
        status: String,
    },
    SessionsStored {
        sessions: Vec<(String, u64)>,
    },
    Error {
        message: String,
    },
}
