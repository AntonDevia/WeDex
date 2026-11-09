use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Yandex,
    Hitmotop,
    SoundCloud,
    YouTube,
    LastFm,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Yandex => "Яндекс Музыка",
            Self::Hitmotop => "Hitmo",
            Self::SoundCloud => "SoundCloud",
            Self::YouTube => "YouTube Music",
            Self::LastFm => "Last.fm",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edition {
    Explicit,
    Clean,
    #[default]
    Unknown,
    ConfirmedOriginal,
}

impl Edition {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explicit => "Explicit · метаданные",
            Self::Clean => "Clean · редакция",
            Self::Unknown => "Версия не проверена",
            Self::ConfirmedOriginal => "Оригинал · подтверждено вами",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub provider: Provider,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub duration: u64,
    pub cover: Option<String>,
    pub page_url: String,
    #[serde(default)]
    pub edition: Edition,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub cover: Option<String>,
    pub tracks: Option<usize>,
    pub listeners: Option<u64>,
}

#[derive(Default)]
pub struct ArtistPage {
    pub artist: Option<Artist>,
    pub tracks: Vec<Track>,
    pub total: Option<usize>,
    pub next_page: Option<u32>,
    pub notices: Vec<String>,
}

#[derive(Clone)]
pub struct ArtistCatalog {
    pub artist: Artist,
    pub tracks: Vec<Track>,
    pub total: Option<usize>,
    pub next_page: Option<u32>,
    pub loading: bool,
    pub failed: bool,
}

impl Track {
    pub fn key(&self) -> String {
        format!("{:?}:{}", self.provider, self.id)
    }
    pub fn identity(&self) -> String {
        format!("{}|{}", normalize(&self.artist), normalize(&self.title))
    }
    pub fn query(&self) -> String {
        format!("{} {}", self.artist, self.title)
    }
}

pub fn normalize(s: &str) -> String {
    s.nfkc()
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn variant(s: &str) -> Vec<&'static str> {
    let text = normalize(s);
    [
        "live",
        "remix",
        "cover",
        "karaoke",
        "instrumental",
        "acoustic",
        "sped",
        "slowed",
        "концерт",
    ]
    .into_iter()
    .filter(|v| text.split_whitespace().any(|w| w == *v))
    .collect()
}

/// Match recording identity without erasing meaningful version qualifiers.
pub fn match_score(seed: &Track, candidate: &Track) -> f64 {
    if variant(&seed.title) != variant(&candidate.title) {
        return 0.0;
    }
    let artist = strsim::jaro_winkler(&normalize(&seed.artist), &normalize(&candidate.artist));
    let title = strsim::jaro_winkler(&normalize(&seed.title), &normalize(&candidate.title));
    if artist < 0.79 || title < 0.79 {
        return 0.0;
    }
    if seed.duration > 0 && candidate.duration > 0 {
        let diff = seed.duration.abs_diff(candidate.duration);
        if diff > 12_u64.max(seed.duration / 14) {
            return 0.0;
        }
    }
    0.45 * artist + 0.55 * title
}

pub fn duration(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn track(title: &str, artist: &str, duration: u64) -> Track {
        Track {
            id: title.into(),
            provider: Provider::Yandex,
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            duration,
            cover: None,
            page_url: String::new(),
            edition: Edition::Unknown,
            reason: String::new(),
            score: 0.0,
        }
    }
    #[test]
    fn rejects_other_recordings() {
        let seed = track("Song", "Artist", 210);
        assert_eq!(
            match_score(&seed, &track("Song (Live)", "Artist", 210)),
            0.0
        );
        assert_eq!(match_score(&seed, &track("Song", "Someone else", 210)), 0.0);
        assert_eq!(match_score(&seed, &track("Song", "Artist", 310)), 0.0);
        assert!(match_score(&seed, &track("SONG", "artist", 212)) > 0.95);
    }
    #[test]
    fn normalization_preserves_cyrillic() {
        assert_eq!(normalize("  Ёлка — ПРОВАНС!"), "ёлка прованс");
    }
}
