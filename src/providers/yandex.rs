use super::*;
use crate::model::ArtistPage;
use serde_json::json;

#[derive(Clone)]
pub struct Playlist {
    pub uid: String,
    pub kind: String,
    pub title: String,
    pub count: u64,
    pub cover: Option<String>,
}

impl Services {
    pub(super) fn ym(&self, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        if self.yandex.is_empty() {
            bail!("Подключите Яндекс Музыку в настройках");
        }
        let value = request_json(
            self.http
                .get(format!("https://api.music.yandex.net/{path}"))
                .header("Authorization", format!("OAuth {}", self.yandex))
                .query(params),
        )?;
        if let Some(error) = value.get("error") {
            bail!("Яндекс: {}", text(error, "message"));
        }
        value
            .get("result")
            .cloned()
            .context("Яндекс вернул ответ без result")
    }
    pub fn yandex_search(&self, query: &str) -> Result<Vec<Track>> {
        let v = self.ym(
            "search",
            &[
                ("text", query),
                ("type", "track"),
                ("page", "0"),
                ("nocorrect", "false"),
            ],
        )?;
        Ok(v.pointer("/tracks/results")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|v| v["available"] != false || v["availableForPremiumUsers"] == true)
                    .filter_map(parse_track)
                    .collect()
            })
            .unwrap_or_default())
    }
    pub fn yandex_search_artists(&self, query: &str) -> Result<Vec<Artist>> {
        let value = self.ym(
            "search",
            &[
                ("text", query),
                ("type", "artist"),
                ("page", "0"),
                ("nocorrect", "false"),
            ],
        )?;
        Ok(value
            .pointer("/artists/results")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(parse_artist).collect())
            .unwrap_or_default())
    }
    pub fn yandex_artist_page(&self, artist_id: &str, page: u32) -> Result<ArtistPage> {
        // A catalog ID must never be allowed to alter the API route.
        if artist_id.is_empty() || !artist_id.bytes().all(|c| c.is_ascii_digit()) {
            bail!("Некорректный идентификатор исполнителя");
        }
        let value = self.ym(
            &format!("artists/{artist_id}/tracks"),
            &[("page", &page.to_string()), ("page-size", "100")],
        )?;
        let mut result = parse_artist_page(&value, page, 100);
        if page == 0 {
            match self.ym(&format!("artists/{artist_id}/brief-info"), &[]) {
                Ok(info) => {
                    result.artist = parse_artist(&info["artist"]);
                    if let Some(artist) = &mut result.artist {
                        artist.listeners = info
                            .pointer("/stats/lastMonthListeners")
                            .and_then(Value::as_u64);
                    }
                }
                Err(error) => result.notices.push(format!("artist-info: {error}")),
            }
        }
        Ok(result)
    }
    pub fn yandex_library(&self) -> Result<(Vec<Track>, Vec<Playlist>, String)> {
        let account = self.ym("account/status", &[])?;
        let uid = id(&account["account"]["uid"]);
        if uid.is_empty() {
            bail!("Яндекс не вернул ID аккаунта");
        }
        let likes = self.ym(&format!("users/{uid}/likes/tracks"), &[])?;
        let ids: Vec<String> = likes
            .pointer("/library/tracks")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|t| id(&t["id"]))
                    .filter(|i| !i.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let mut tracks = Vec::new();
        for batch in ids.chunks(100) {
            let joined = batch.join(",");
            let value = request_json(
                self.http
                    .post("https://api.music.yandex.net/tracks")
                    .header("Authorization", format!("OAuth {}", self.yandex))
                    .form(&[("track-ids", joined)]),
            )?;
            let items = value["result"]
                .as_array()
                .context("Не удалось получить метаданные понравившихся")?;
            tracks.extend(items.iter().filter_map(parse_track));
        }
        let list = self.ym(&format!("users/{uid}/playlists/list"), &[])?;
        let playlists = list
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|p| Playlist {
                        uid: uid.clone(),
                        kind: id(&p["kind"]),
                        title: text(p, "title"),
                        count: p["trackCount"].as_u64().unwrap_or(0),
                        cover: p["cover"]["uri"]
                            .as_str()
                            .or_else(|| p["cover"]["itemsUri"].as_array()?.first()?.as_str())
                            .map(|uri| {
                                format!(
                                    "https://{}",
                                    uri.trim_start_matches("https://").replace("%%", "400x400")
                                )
                            }),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let name = text(&account["account"], "displayName");
        Ok((tracks, playlists, name))
    }
    pub fn yandex_playlist(&self, playlist: &Playlist) -> Result<Vec<Track>> {
        let value = self.ym(
            &format!("users/{}/playlists/{}", playlist.uid, playlist.kind),
            &[],
        )?;
        let mut tracks = Vec::new();
        let mut ids = Vec::new();
        if let Some(items) = value.get("tracks").and_then(Value::as_array) {
            for item in items {
                if let Some(t) = item.get("track").and_then(parse_track) {
                    tracks.push(t);
                } else {
                    let value = id(&item["id"]);
                    if !value.is_empty() {
                        ids.push(value);
                    }
                }
            }
        }
        for batch in ids.chunks(100) {
            let value = self.ym("tracks", &[("track-ids", &batch.join(","))])?;
            if let Some(items) = value.as_array() {
                tracks.extend(items.iter().filter_map(parse_track));
            }
        }
        Ok(tracks)
    }
    pub fn yandex_stream(&self, track: &Track) -> Result<String> {
        let value = self.ym(&format!("tracks/{}/download-info", track.id), &[])?;
        let info = value
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .filter(|v| v["codec"] == "mp3" && v["preview"] != json!(true))
                    .max_by_key(|v| v["bitrateInKbps"].as_u64().unwrap_or(0))
            })
            .context(
                "Нет полного MP3-потока. Проверьте подписку и доступность трека для аккаунта",
            )?;
        let xml_url = text(info, "downloadInfoUrl");
        let xml = request(self.http.get(xml_url))?
            .text()
            .map_err(|e| e.without_url())?;
        let doc = roxmltree::Document::parse(&xml)?;
        let field = |name: &str| -> Result<&str> {
            doc.descendants()
                .find(|n| n.has_tag_name(name))
                .and_then(|n| n.text())
                .context("Неполные данные аудиопотока Яндекса")
        };
        let path = field("path")?;
        let sign = format!(
            "{:x}",
            md5::compute(format!(
                "XGRlBW9FXlekgbPrRHuSiA{}{}",
                path.trim_start_matches('/'),
                field("s")?
            ))
        );
        Ok(format!(
            "https://{}/get-mp3/{}/{}{}",
            field("host")?,
            sign,
            field("ts")?,
            path
        ))
    }
}

fn parse_artist(v: &Value) -> Option<Artist> {
    let artist_id = id(&v["id"]);
    let name = text(v, "name");
    if artist_id.is_empty() || name.is_empty() {
        return None;
    }
    Some(Artist {
        id: artist_id,
        name,
        cover: v
            .pointer("/cover/uri")
            .and_then(Value::as_str)
            .or_else(|| v["ogImage"].as_str())
            .filter(|s| !s.is_empty())
            .map(|uri| {
                format!(
                    "https://{}",
                    uri.trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .replace("%%", "400x400")
                )
            }),
        tracks: v
            .pointer("/counts/tracks")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        listeners: None,
    })
}

fn parse_artist_page(value: &Value, page: u32, requested: usize) -> ArtistPage {
    let items = value["tracks"].as_array().cloned().unwrap_or_default();
    let total = value
        .pointer("/pager/total")
        .and_then(Value::as_u64)
        .map(|n| n as usize);
    let per_page = value
        .pointer("/pager/perPage")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .filter(|n| *n > 0)
        .unwrap_or(requested);
    let more = !items.is_empty()
        && total.map_or(items.len() >= per_page, |n| {
            (page as usize + 1) * per_page < n
        });
    ArtistPage {
        tracks: items
            .iter()
            .filter(|v| v["available"] != false || v["availableForPremiumUsers"] == true)
            .filter_map(parse_track)
            .collect(),
        total,
        next_page: more.then_some(page + 1),
        ..Default::default()
    }
}

pub fn parse_track(v: &Value) -> Option<Track> {
    let track_id = id(&v["id"]);
    let title = text(v, "title");
    if track_id.is_empty() || title.is_empty() {
        return None;
    }
    let artists = v["artists"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|i| text(i, "name"))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let version = text(v, "version");
    let full_title = if version.is_empty() {
        title
    } else {
        format!("{title} ({version})")
    };
    let content = text(v, "contentWarning");
    let edition = if content == "explicit" {
        Edition::Explicit
    } else if version.to_lowercase().contains("clean")
        || version.to_lowercase().contains("radio edit")
    {
        Edition::Clean
    } else {
        Edition::Unknown
    };
    let cover = v["coverUri"].as_str().map(|c| {
        format!(
            "https://{}",
            c.trim_start_matches("https://").replace("%%", "600x600")
        )
    });
    Some(Track {
        id: track_id.clone(),
        provider: Provider::Yandex,
        title: full_title,
        artist: artists,
        album: v["albums"]
            .as_array()
            .and_then(|albums| albums.first())
            .map(|album| text(album, "title"))
            .unwrap_or_default(),
        duration: v["durationMs"].as_u64().unwrap_or(0) / 1000,
        cover,
        page_url: format!("https://music.yandex.ru/track/{track_id}"),
        edition,
        reason: String::new(),
        score: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artist_identity_and_photo_come_from_artist_metadata() {
        let artist = parse_artist(&json!({"id":42,"name":"Artist", "cover":{"uri":"avatars.yandex.net/artist/%%"}, "counts":{"tracks":201}})).unwrap();
        assert_eq!(artist.id, "42");
        assert_eq!(artist.tracks, Some(201));
        assert_eq!(
            artist.cover.as_deref(),
            Some("https://avatars.yandex.net/artist/400x400")
        );
        assert!(parse_artist(&json!({"id":42})).is_none());
    }
    #[test]
    fn pagination_uses_raw_page_and_total_even_when_tracks_are_unavailable() {
        let value = json!({"tracks":[{"id":1,"title":"Hidden","available":false}, {"id":2,"title":"Song","available":true}],"pager":{"total":5,"perPage":2}});
        let first = parse_artist_page(&value, 0, 100);
        assert_eq!(first.tracks.len(), 1);
        assert_eq!(first.next_page, Some(1));
        assert_eq!(first.total, Some(5));
        assert_eq!(parse_artist_page(&value, 2, 100).next_page, None);
        let no_pager = json!({"tracks":[{"id":1,"title":"Song"}, {"id":2,"title":"Other"}]});
        assert_eq!(parse_artist_page(&no_pager, 0, 2).next_page, Some(1));
        assert_eq!(
            parse_artist_page(&json!({"tracks":[]}), 1, 2).next_page,
            None
        );
    }
    #[test]
    #[ignore = "Calls the live Yandex artist search and paginated catalog with the configured token"]
    fn live_artist_catalog() {
        let services = Services::new(crate::store::secret("yandex"), String::new(), String::new());
        let artists = services.yandex_search_artists("The Weeknd").unwrap();
        let artist = artists.first().expect("artist search returned a result");
        assert!(artist.cover.is_some());
        let first = services.yandex_artist_page(&artist.id, 0).unwrap();
        assert!(!first.tracks.is_empty());
        assert!(first.artist.is_some());
        println!(
            "Artist catalog: {} tracks on first page, total {:?}, next {:?}",
            first.tracks.len(),
            first.total,
            first.next_page
        );
        if let Some(page) = first.next_page {
            let second = services.yandex_artist_page(&artist.id, page).unwrap();
            assert!(!second.tracks.is_empty());
            assert_ne!(first.tracks[0].key(), second.tracks[0].key());
        }
    }
    #[test]
    fn yandex_explicit_is_metadata_only() {
        let track = parse_track(&json!({"id":123,"title":"Song","durationMs":180000,"artists":[{"name":"Artist"}],"contentWarning":"explicit"})).unwrap();
        assert_eq!(track.edition, Edition::Explicit);
        assert_eq!(track.duration, 180);
        assert_ne!(track.edition, Edition::ConfirmedOriginal);
    }
}
