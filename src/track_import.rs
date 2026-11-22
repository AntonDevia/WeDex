use crate::{
    model::{normalize, Track},
    providers::Services,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
        Arc, Mutex,
    },
};

#[cfg(any(windows, target_os = "android"))]
pub mod file;

pub const FILE_LIMIT: usize = 8 * 1024 * 1024;
const TRACK_LIMIT: usize = 10_000;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Spotify,
    Yandex,
    Vk,
}

impl Source {
    pub const ALL: [Self; 3] = [Self::Spotify, Self::Yandex, Self::Vk];

    pub fn label(self) -> &'static str {
        match self {
            Self::Spotify => "Spotify",
            Self::Yandex => "Яндекс Музыка",
            Self::Vk => "VK Музыка",
        }
    }
}

#[derive(Clone)]
pub enum Input {
    Text(Source, String),
    YandexLikes,
}

#[derive(Debug)]
pub struct Song {
    title: String,
    artist: String,
}

impl Song {
    pub fn new(title: &str, artist: &str) -> Option<Self> {
        let title = title.trim();
        let artist = artist.trim();
        if title.chars().count() > 400
            || artist.chars().count() > 400
            || normalize(title).is_empty()
            || title.contains("://")
            || artist.contains("://")
        {
            return None;
        }
        Some(Self {
            title: title.into(),
            artist: artist.into(),
        })
    }

    pub fn label(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} — {}", self.artist, self.title)
        }
    }
}

#[derive(Default)]
pub struct Progress {
    pub total: AtomicUsize,
    pub done: AtomicUsize,
    pub cancelled: AtomicBool,
}

#[derive(Default)]
pub struct Report {
    pub added: usize,
    pub existing: usize,
    pub hidden: usize,
    pub missing: Vec<Song>,
    pub unchecked: Vec<Song>,
    pub ambiguous: Vec<Song>,
}

#[derive(Clone, Default)]
pub enum View {
    #[default]
    Idle,
    Picking,
    Working(Arc<Progress>),
    Finished(Arc<Report>),
    Error(String),
}

impl View {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Picking | Self::Working(_))
    }
}

fn field<'a>(item: &'a Value, names: &[&str]) -> &'a str {
    names
        .iter()
        .find_map(|name| item[*name].as_str())
        .unwrap_or_default()
}

fn array(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn spotify(value: &Value) -> Vec<Song> {
    let playlist_tracks = array(&value["playlists"])
        .flat_map(|playlist| array(&playlist["items"]))
        .map(|item| &item["track"]);
    playlist_tracks
        .chain(array(&value["tracks"]))
        .filter_map(|track| {
            Song::new(
                field(track, &["trackName", "track"]),
                field(track, &["artistName", "artist"]),
            )
        })
        .collect()
}

fn vk_json(value: &Value) -> Vec<Song> {
    array(value)
        .chain(array(&value["response"]["items"]))
        .chain(array(&value["items"]))
        .filter_map(|item| Song::new(field(item, &["title"]), field(item, &["artist"])))
        .filter(|song| !song.artist.is_empty())
        .collect()
}

fn vk_html(contents: &str) -> Vec<Song> {
    use scraper::{Html, Selector};
    let html = Html::parse_document(contents);
    let rows = Selector::parse(".audio_row").unwrap();
    let titles = Selector::parse(".audio_row__title_inner, .audio_row__title a").unwrap();
    let artists = Selector::parse(".audio_row__performers a").unwrap();
    let links = Selector::parse("a[href]").unwrap();
    let mut songs: Vec<_> = html
        .select(&rows)
        .filter_map(|row| {
            let title = row.select(&titles).next()?.text().collect::<String>();
            let artist = row
                .select(&artists)
                .map(|node| node.text().collect::<String>())
                .collect::<Vec<_>>()
                .join(", ");
            Song::new(&title, &artist).filter(|song| !song.artist.is_empty())
        })
        .collect();
    if songs.is_empty() {
        // Archived music entries may be plain track links rather than player rows.
        songs.extend(html.select(&links).filter_map(|link| {
            let href = link.attr("href")?;
            let path = href.strip_prefix("https://vk.com").unwrap_or(href);
            let id = path.strip_prefix("/audio")?.split(['?', '#']).next()?;
            let (owner, track) = id.split_once('_')?;
            owner.parse::<i64>().ok()?;
            track.parse::<u64>().ok()?;
            let label = link.text().collect::<String>();
            let (artist, title) = split_song(&label)?;
            Song::new(title, artist)
        }));
    }
    songs
}

fn split_song(line: &str) -> Option<(&str, &str)> {
    [" — ", " – ", " - ", "\t"]
        .into_iter()
        .find_map(|separator| line.split_once(separator))
}

fn csv(contents: &str) -> Result<Option<Vec<Song>>> {
    for &delimiter in b",;\t" {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .from_reader(contents.as_bytes());
        let Ok(headers) = reader.headers() else {
            continue;
        };
        let name = |names: &[&str]| {
            headers
                .iter()
                .position(|v| names.contains(&normalize(v).as_str()))
        };
        let Some(title) = name(&[
            "track",
            "track name",
            "title",
            "track title",
            "название",
            "название трека",
            "трек",
        ]) else {
            continue;
        };
        let Some(artist) = name(&[
            "artist",
            "artist name",
            "artist names",
            "artist name s",
            "исполнитель",
            "исполнители",
        ]) else {
            continue;
        };
        return reader
            .records()
            .map(|row| {
                let row = row.context("Не удалось прочитать строку CSV")?;
                Ok(Song::new(&row[title], &row[artist]))
            })
            .filter_map(Result::transpose)
            .collect::<Result<Vec<_>>>()
            .map(Some);
    }
    Ok(None)
}

fn parse(source: Source, contents: &str) -> Result<Vec<Song>> {
    let contents = contents.trim_start_matches('\u{feff}').trim();
    if contents.len() > FILE_LIMIT {
        bail!("Размер списка превышает 8 МБ");
    }
    if contents.contains('\0') {
        bail!("Выберите текстовый файл с названиями треков");
    }
    if contents.starts_with(['{', '[']) {
        let value: Value = serde_json::from_str(contents).context("Не удалось прочитать JSON")?;
        return Ok(match source {
            Source::Spotify => spotify(&value),
            Source::Vk => vk_json(&value),
            Source::Yandex => bail!("Для Яндекс Музыки используйте ссылку, CSV или TXT"),
        });
    }
    if contents.starts_with('<') {
        return match source {
            Source::Vk => Ok(vk_html(contents)),
            _ => bail!("Этот источник не поддерживает импорт HTML"),
        };
    }
    if let Some(songs) = csv(contents)? {
        return Ok(songs);
    }
    let mut songs = Vec::new();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let (artist, title) = split_song(line).unwrap_or(("", line));
        let Some(song) = Song::new(title, artist) else {
            bail!("Проверьте список. Укажите по одному треку на строку");
        };
        songs.push(song);
        if songs.len() > TRACK_LIMIT {
            bail!("В одном списке может быть до {TRACK_LIMIT} треков");
        }
    }
    Ok(songs)
}

fn yandex_url(contents: &str) -> Result<String> {
    let url = url::Url::parse(contents).context("Вставьте ссылку на плейлист Яндекс Музыки")?;
    anyhow::ensure!(
        url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some(
                    "music.yandex.ru" | "music.yandex.com" | "music.yandex.kz" | "music.yandex.by"
                )
            )
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        "Вставьте ссылку с сайта Яндекс Музыки"
    );
    let parts: Vec<_> = url.path().trim_end_matches('/').split('/').collect();
    match parts.as_slice() {
        ["", "playlists", key] => {
            let id = key
                .strip_prefix("lk.")
                .or_else(|| key.strip_prefix("ch."))
                .unwrap_or(key);
            if uuid::Uuid::try_parse(id).is_ok() {
                return Ok(format!("playlist/{key}"));
            }
        }
        ["", "users", user, "playlists", kind] | ["", "iframe", "playlist", user, kind]
            if !user.is_empty()
                && user
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
                && !matches!(*user, "." | "..")
                && !kind.is_empty()
                && kind.bytes().all(|c| c.is_ascii_digit()) =>
        {
            return Ok(format!("users/{user}/playlists/{kind}"));
        }
        _ => {}
    }
    bail!("Вставьте полную ссылку на плейлист Яндекс Музыки");
}

enum Match {
    Found(Track),
    Existing,
    Missing,
    Unchecked,
    Ambiguous,
}

fn select(song: &Song, candidates: Vec<Track>, incomplete: bool) -> Match {
    let title = normalize(&song.title);
    let artist = normalize(&song.artist);
    let mut exact = candidates.into_iter().filter(|track| {
        normalize(&track.title) == title
            && (artist.is_empty() || normalize(&track.artist) == artist)
    });
    let Some(first) = exact.next() else {
        return if incomplete {
            Match::Unchecked
        } else {
            Match::Missing
        };
    };
    if artist.is_empty() && exact.any(|track| normalize(&track.artist) != normalize(&first.artist))
    {
        return Match::Ambiguous;
    }
    Match::Found(first)
}

pub fn run(
    services: &Services,
    input: Input,
    offline: bool,
    favorites: Vec<Track>,
    progress: &Progress,
) -> Result<(Vec<Track>, Report)> {
    let mut songs = match input {
        Input::Text(Source::Yandex, text) if text.trim().starts_with("https://") => {
            anyhow::ensure!(!offline, "Для чтения плейлиста нужен интернет");
            let path = yandex_url(text.trim())?;
            services.yandex_playlist_metadata(&path)?
        }
        Input::Text(source, text) => parse(source, &text)?,
        Input::YandexLikes => {
            anyhow::ensure!(!offline, "Для чтения понравившихся Яндекса нужен интернет");
            services.yandex_liked_metadata()?
        }
    };
    let mut seen = HashSet::new();
    songs.retain(|song| seen.insert((normalize(&song.artist), normalize(&song.title))));
    if songs.is_empty() {
        bail!("В списке нет треков. Выберите файл с названиями и исполнителями");
    }
    if songs.len() > TRACK_LIMIT {
        bail!("В одном списке может быть до {TRACK_LIMIT} треков");
    }
    progress.total.store(songs.len(), Relaxed);
    let known: HashSet<_> = favorites
        .into_iter()
        .map(|track| (normalize(&track.artist), normalize(&track.title)))
        .collect();
    let pending = Mutex::new(songs.into_iter().enumerate());
    let completed = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..3 {
            scope.spawn(|| {
                while !progress.cancelled.load(Relaxed) {
                    let Some((index, song)) = pending.lock().unwrap().next() else {
                        break;
                    };
                    let key = (normalize(&song.artist), normalize(&song.title));
                    let result = if known.contains(&key) {
                        Match::Existing
                    } else if offline {
                        Match::Unchecked
                    } else {
                        let query = format!("{} {}", song.artist, song.title);
                        let (result, completed) = services.search_all_tracks(query.trim());
                        select(&song, result.tracks, !completed)
                    };
                    completed.lock().unwrap().push((index, song, result));
                    progress.done.fetch_add(1, Relaxed);
                }
            });
        }
    });
    let mut completed = completed.into_inner().unwrap();
    completed.sort_by_key(|(index, _, _)| *index);
    let mut tracks = Vec::new();
    let mut report = Report::default();
    for (_, song, result) in completed {
        match result {
            Match::Found(track) => tracks.push(track),
            Match::Existing => report.existing += 1,
            Match::Missing => report.missing.push(song),
            Match::Unchecked => report.unchecked.push(song),
            Match::Ambiguous => report.ambiguous.push(song),
        }
    }
    Ok((tracks, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::track;

    #[test]
    fn reads_spotify_library_and_playlist_without_non_music_entries() {
        for text in [
            r#"{"tracks":[{"artist":"Björk","track":"Jóga"}],"albums":[{"name":"Homogenic"}]}"#,
            r#"{"playlists":[{"items":[{"episode":{"name":"Talk"}},{"track":{"artistName":"Björk","trackName":"Jóga"}}]}]}"#,
        ] {
            let songs = parse(Source::Spotify, text).unwrap();
            assert_eq!(songs.len(), 1);
            assert_eq!(songs[0].label(), "Björk — Jóga");
        }
    }

    #[test]
    fn reads_vk_saved_track_links_and_quoted_csv_metadata() {
        let html = r#"<a href="https://vk.com/audio-5_8">Artist &amp; Friend — Song</a><a href="/video-5_8">Other — Video</a>"#;
        let songs = parse(Source::Vk, html).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].label(), "Artist & Friend — Song");
        let songs = parse(
            Source::Vk,
            "Track Name,Artist Name(s)\r\n\"Hello, Goodbye\",The Beatles",
        )
        .unwrap();
        assert_eq!(songs[0].label(), "The Beatles — Hello, Goodbye");
    }

    #[test]
    fn verifies_yandex_playlist_origin_and_path() {
        assert_eq!(
            yandex_url("https://music.yandex.ru/users/test.7/playlists/3?x=1").unwrap(),
            "users/test.7/playlists/3"
        );
        assert_eq!(
            yandex_url("https://music.yandex.ru/playlists/ch.a198c142-e6f5-c3cd-959e-c691ae26995f")
                .unwrap(),
            "playlist/ch.a198c142-e6f5-c3cd-959e-c691ae26995f"
        );
        assert!(yandex_url("https://music.yandex.ru.example.org/users/test/playlists/3").is_err());
        assert!(yandex_url("https://music.yandex.ru/users/test%2Fetc/playlists/3").is_err());
    }

    #[test]
    fn keeps_remixes_and_title_only_ambiguity_out_of_favorites() {
        let song = Song::new("Time", "Artist").unwrap();
        assert!(matches!(
            select(&song, vec![track("Time (Remix)", "Artist", 123)], false),
            Match::Missing
        ));
        assert!(matches!(
            select(&song, vec![track("Time", "Other", 123)], true),
            Match::Unchecked
        ));
        assert!(matches!(
            select(&song, vec![track("TIME", "artist", 123)], false),
            Match::Found(_)
        ));
        let song = Song::new("Time", "").unwrap();
        assert!(matches!(
            select(
                &song,
                vec![track("Time", "Artist", 123), track("Time", "Other", 123)],
                false
            ),
            Match::Ambiguous
        ));
    }
}
