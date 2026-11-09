use super::*;
use scraper::{Html, Selector};
use serde_json::json as j;

#[cfg(test)]
const HITMO_BASE: &str = super::connections::DEFAULT_HITMO;

fn edition(title: &str) -> Edition {
    let lower = title.to_lowercase();
    if lower.contains("uncensored") || lower.contains("explicit") || lower.contains("без цензуры")
    {
        Edition::Explicit
    } else if lower.contains("clean version") || lower.contains("radio edit") {
        Edition::Clean
    } else {
        Edition::Unknown
    }
}
fn time(s: &str) -> u64 {
    s.trim()
        .split(':')
        .try_fold(0_u64, |n, s| Some(n * 60 + s.parse::<u64>().ok()?))
        .unwrap_or(0)
}

impl Services {
    pub fn hitmotop_search(&self, query: &str) -> Result<Vec<Track>> {
        let page = request(
            self.http
                .get(format!("{}search", self.hitmo_base))
                .query(&[("q", query)]),
        )?;
        let base = page.url().clone();
        let body = page.text().map_err(|e| e.without_url())?;
        parse_hitmotop(&body, &base)
    }
    pub fn hitmotop_stream(&self, track: &Track) -> Result<String> {
        let page = request(self.http.get(self.hitmo_page_url(&track.page_url)?))?;
        let base = page.url().clone();
        if page
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("audio/"))
        {
            return Ok(base.to_string());
        }
        let body = page.text().map_err(|e| e.without_url())?;
        let song_id = url::Url::parse(&track.page_url)
            .ok()
            .and_then(|u| u.path_segments()?.next_back().map(str::to_owned));
        if let Some((_, stream)) = modern_hitmo(&body, &base)
            .into_iter()
            .find(|(t, _)| t.id == track.id || song_id.as_deref() == Some(t.id.as_str()))
        {
            return Ok(stream);
        }
        let html = Html::parse_document(&body);
        // Prefer the song's download link; unrelated recommendations follow it on the page.
        for a in html.select(&Selector::parse("a.track__download-btn[href], a.p-track-download__btn[href], audio[src], source[src]").unwrap()) {
            if let Some(href) = a.value().attr("href").or_else(|| a.value().attr("src")) {
                if href.contains(".mp3") || href.contains("/get/") {
                    return Ok(base.join(href)?.to_string());
                }
            }
        }
        bail!("Hitmo не вернул аудио для этой записи")
    }
    pub fn soundcloud_search(&self, query: &str) -> Result<Vec<Track>> {
        if self.soundcloud.is_empty() {
            bail!("Нужен OAuth access token SoundCloud в настройках");
        }
        let value = request_json(
            self.http
                .get("https://api.soundcloud.com/tracks")
                .header("Authorization", format!("OAuth {}", self.soundcloud))
                .query(&[
                    ("q", query),
                    ("limit", "20"),
                    ("linked_partitioning", "true"),
                ]),
        )?;
        let items = value
            .get("collection")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .context("Неизвестный ответ SoundCloud")?;
        Ok(items
            .iter()
            .filter_map(|v| {
                let track_id = v["urn"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| id(&v["id"]));
                if track_id.is_empty() {
                    return None;
                }
                let title = text(v, "title");
                Some(Track {
                    album: String::new(),
                    id: track_id,
                    provider: Provider::SoundCloud,
                    edition: edition(&title),
                    title,
                    artist: v["metadata_artist"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .unwrap_or_else(|| text(&v["user"], "username")),
                    duration: v["duration"].as_u64().unwrap_or(0) / 1000,
                    cover: v["artwork_url"].as_str().map(str::to_owned),
                    page_url: text(v, "permalink_url"),
                    reason: String::new(),
                    score: 0.0,
                })
            })
            .collect())
    }
    pub fn soundcloud_stream(&self, track: &Track) -> Result<String> {
        if self.soundcloud.is_empty() {
            bail!("Подключите SoundCloud");
        }
        // Progressive URLs were removed in 2026. Use full-track MP3 HLS, never the preview field.
        let value = request_json(
            self.http
                .get(format!(
                    "https://api.soundcloud.com/tracks/{}/streams",
                    track.id
                ))
                .header("Authorization", format!("OAuth {}", self.soundcloud)),
        )?;
        for key in ["hls_mp3_128_url", "http_mp3_128_url"] {
            if let Some(url) = value[key].as_str() {
                return Ok(url.to_owned());
            }
        }
        bail!("SoundCloud не предоставил MP3 HLS. AAC HLS пока не поддерживается; откройте запись в сервисе")
    }
    pub fn youtube_search(&self, query: &str) -> Result<Vec<Track>> {
        let value = request_json(self.http.post("https://music.youtube.com/youtubei/v1/search").json(&j!({"context":{"client":{"clientName":"WEB_REMIX","clientVersion":"1.20261026.01.00","hl":"ru","gl":"US"}},"query":query,"params":"EgWKAQIIAWoKEAMQBBAJEAoQBQ%3D%3D"})))?;
        let mut tracks = Vec::new();
        parse_youtube(&value, &mut tracks);
        let mut seen = HashSet::new();
        tracks.retain(|t| seen.insert(t.id.clone()));
        tracks.truncate(25);
        Ok(tracks)
    }
    pub fn youtube_stream(&self, track: &Track) -> Result<String> {
        let value = request_json(self.http.post("https://music.youtube.com/youtubei/v1/player").json(&j!({"context":{"client":{"clientName":"WEB_REMIX","clientVersion":"1.20261026.01.00"}},"videoId":track.id})))?;
        if let Some(formats) = value
            .pointer("/streamingData/adaptiveFormats")
            .and_then(Value::as_array)
        {
            if let Some(format) = formats
                .iter()
                .filter(|f| {
                    f["mimeType"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("audio/mp4"))
                        && f["url"].is_string()
                })
                .max_by_key(|f| f["bitrate"].as_u64().unwrap_or(0))
            {
                return Ok(text(format, "url"));
            }
        }
        let reason = value
            .pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("сервис требует защищённый веб-плеер");
        bail!("YouTube Music: {reason}. Можно открыть запись в сервисе; защищённые потоки пока не поддерживаются")
    }
}

fn parse_hitmotop(body: &str, base: &url::Url) -> Result<Vec<Track>> {
    let html = Html::parse_document(body);
    let item = Selector::parse(".tracks__item").unwrap();
    let title = Selector::parse(".track__title").unwrap();
    let artist = Selector::parse(".track__desc").unwrap();
    let duration = Selector::parse(".track__fulltime").unwrap();
    let link = Selector::parse("a.track__info-l[href], a.track__download-btn[href]").unwrap();
    let mut tracks = Vec::new();
    for row in html.select(&item).take(25) {
        let get = |selector: &Selector| {
            row.select(selector)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_owned())
                .unwrap_or_default()
        };
        let name = get(&title);
        let Some(href) = row
            .select(&link)
            .next()
            .and_then(|a| a.value().attr("href"))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let page = base.join(href)?.to_string();
        let meta = row
            .value()
            .attr("data-musmeta")
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        let cover = meta
            .as_ref()
            .and_then(|m| m["img"].as_str())
            .filter(|s| !s.is_empty())
            .and_then(|s| base.join(s).ok())
            .map(|u| u.to_string());
        tracks.push(Track {
            album: String::new(),
            id: row
                .value()
                .attr("data-musid")
                .map(str::to_owned)
                .unwrap_or_else(|| page.clone()),
            provider: Provider::Hitmotop,
            title: name.clone(),
            artist: get(&artist),
            duration: time(&get(&duration)),
            cover,
            page_url: page,
            edition: edition(&name),
            reason: String::new(),
            score: 0.0,
        });
    }
    for (track, _) in modern_hitmo(body, base) {
        if !tracks.iter().any(|t| t.id == track.id) {
            tracks.push(track);
        }
    }
    tracks.truncate(50);
    if tracks.is_empty() && (body.contains("captcha") || body.contains("Access denied")) {
        bail!("Сервис требует проверку в браузере");
    }
    Ok(tracks)
}

fn modern_hitmo(body: &str, base: &url::Url) -> Vec<(Track, String)> {
    fn walk(v: &Value, base: &url::Url, out: &mut Vec<(Track, String)>) {
        if let Some(o) = v.as_object() {
            if let (Some(title), Some(artist), Some(play)) = (
                o.get("title").and_then(Value::as_str),
                o.get("artist").and_then(Value::as_str),
                o.get("play").and_then(Value::as_str),
            ) {
                let song_id = o.get("id").map(id).unwrap_or_default();
                if !song_id.is_empty() && !title.is_empty() {
                    if let Ok(stream) = base.join(play) {
                        if matches!(stream.scheme(), "https" | "http")
                            && !out.iter().any(|(t, _)| t.id == song_id)
                        {
                            out.push((
                                Track {
                                    id: song_id.clone(),
                                    provider: Provider::Hitmotop,
                                    title: title.into(),
                                    artist: artist.into(),
                                    album: String::new(),
                                    duration: o
                                        .get("duration")
                                        .and_then(Value::as_u64)
                                        .unwrap_or(0),
                                    cover: o
                                        .get("image")
                                        .and_then(Value::as_str)
                                        .and_then(|s| base.join(s).ok())
                                        .map(|u| u.to_string()),
                                    page_url: base
                                        .join(&format!("/song/{song_id}"))
                                        .unwrap()
                                        .to_string(),
                                    edition: edition(title),
                                    reason: String::new(),
                                    score: 0.,
                                },
                                stream.to_string(),
                            ));
                        }
                    }
                }
            }
            for value in o.values() {
                walk(value, base, out);
            }
        } else if let Some(a) = v.as_array() {
            for value in a {
                walk(value, base, out);
            }
        }
    }
    let mut flight = String::new();
    for part in body.split("self.__next_f.push(").skip(1) {
        if let Some(Ok(value)) = serde_json::Deserializer::from_str(part)
            .into_iter::<Value>()
            .next()
        {
            if let Some(text) = value.get(1).and_then(Value::as_str) {
                flight.push_str(text);
            }
        }
    }
    let mut out = vec![];
    for line in flight.lines() {
        if let Some((_, json)) = line.split_once(':') {
            if let Ok(value) = serde_json::from_str::<Value>(json) {
                walk(&value, base, &mut out);
            }
        }
    }
    out
}

fn parse_youtube(v: &Value, out: &mut Vec<Track>) {
    if let Some(row) = v.get("musicResponsiveListItemRenderer") {
        let columns = row["flexColumns"].as_array();
        let runs = |i: usize| -> Vec<&Value> {
            columns
                .and_then(|c| c.get(i))
                .and_then(|c| c.pointer("/musicResponsiveListItemFlexColumnRenderer/text/runs"))
                .and_then(Value::as_array)
                .map(|a| a.iter().collect())
                .unwrap_or_default()
        };
        let first = runs(0);
        let track_id = row
            .pointer("/playlistItemData/videoId")
            .and_then(Value::as_str)
            .or_else(|| {
                first
                    .first()
                    .and_then(|r| r.pointer("/navigationEndpoint/watchEndpoint/videoId"))
                    .and_then(Value::as_str)
            });
        if let Some(track_id) = track_id {
            let title = first.iter().map(|r| text(r, "text")).collect::<String>();
            let second = runs(1);
            let artist = second.first().map(|r| text(r, "text")).unwrap_or_default();
            let duration = second
                .iter()
                .rev()
                .find_map(|r| r["text"].as_str().filter(|s| s.contains(':')).map(time))
                .unwrap_or(0);
            let cover = row
                .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
                .and_then(Value::as_array)
                .and_then(|a| a.last())
                .and_then(|t| t["url"].as_str())
                .map(str::to_owned);
            if !title.is_empty() {
                out.push(Track {
                    album: String::new(),
                    id: track_id.to_owned(),
                    provider: Provider::YouTube,
                    edition: edition(&title),
                    title,
                    artist,
                    duration,
                    cover,
                    page_url: format!("https://music.youtube.com/watch?v={track_id}"),
                    reason: String::new(),
                    score: 0.0,
                });
            }
        }
        return;
    }
    match v {
        Value::Object(map) => {
            for child in map.values() {
                parse_youtube(child, out);
            }
        }
        Value::Array(array) => {
            for child in array {
                parse_youtube(child, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_hitmo_reads_next_data_and_preserves_exact_song_identity() {
        let tracks = j!({"tracks":[{"id":10,"title":"Fixture","artist":"Artist","duration":123,"play":"https://plc1.hitmos.fm/fixture.mp3","download":"https://dwc1.hitmos.fm/fixture.mp3","image":"/cover.webp"},{"id":11,"title":"Recommended","artist":"Other","duration":42,"play":"https://plc1.hitmos.fm/other.mp3"}]});
        let flight = format!("a:{tracks}\n");
        let mid = flight.len() / 2;
        let html = format!(
            "<script>self.__next_f.push({});self.__next_f.push({})</script>",
            j!([1, &flight[..mid]]),
            j!([1, &flight[mid..]])
        );
        let base = url::Url::parse("https://rus.hitmos.fm/search?q=fixture").unwrap();
        let parsed = parse_hitmotop(&html, &base).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "10");
        assert_eq!(parsed[0].duration, 123);
        assert_eq!(parsed[0].page_url, "https://rus.hitmos.fm/song/10");
        assert_eq!(
            modern_hitmo(&html, &base)
                .into_iter()
                .find(|(t, _)| t.id == "10")
                .unwrap()
                .1,
            "https://plc1.hitmos.fm/fixture.mp3"
        );
    }
    #[test]
    #[ignore = "Live new Hitmo markup and first audio bytes"]
    fn live_modern_hitmo_search_and_audio() {
        use std::io::Read;
        let mut services = Services::new(String::new(), String::new(), String::new());
        services.hitmo_base = "https://rus.hitmos.fm/".into();
        let tracks = services.hitmotop_search("Kambulat").unwrap();
        assert!(!tracks.is_empty());
        let stream = services.hitmotop_stream(&tracks[0]).unwrap();
        let response = request(services.http.get(stream).header("Range", "bytes=0-8191")).unwrap();
        assert!(response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("audio/"));
        let mut bytes = vec![];
        response.take(8192).read_to_end(&mut bytes).unwrap();
        assert!(
            bytes.starts_with(b"ID3") || bytes.windows(2).any(|b| b[0] == 255 && b[1] & 224 == 224)
        );
        println!(
            "Modern Hitmo: {} tracks; {} MP3 bytes checked",
            tracks.len(),
            bytes.len()
        );
    }
    #[test]
    fn hitmotop_keeps_unknown_status() {
        let fixture = r#"<li class="tracks__item"><div class="track__title">Песня</div><div class="track__desc">Артист</div><div class="track__fulltime">3:25</div><a class="track__download-btn" href="/get/song.mp3"></a></li>"#;
        let t = parse_hitmotop(fixture, &url::Url::parse(HITMO_BASE).unwrap())
            .unwrap()
            .remove(0);
        assert_eq!(t.duration, 205);
        assert_eq!(t.edition, Edition::Unknown);
        assert_eq!(t.page_url, "https://ru.hitmo-top.cc/get/song.mp3");
    }
    #[test]
    fn hitmo_preserves_song_page_and_metadata_cover() {
        let fixture = r#"<li class="tracks__item" data-musid="track-id-123" data-musmeta='{"img":"//statcore.hitmcdn.com/cover.jpg"}'><a class="track__info-l" href="/song/123"><div class="track__title">Song</div><div class="track__desc">Artist</div></a><div class="track__fulltime">05:30</div><a class="track__download-btn" href="/get/song.mp3"></a></li>"#;
        let track = parse_hitmotop(fixture, &url::Url::parse(HITMO_BASE).unwrap())
            .unwrap()
            .remove(0);
        assert_eq!(track.id, "track-id-123");
        assert_eq!(track.page_url, "https://ru.hitmo-top.cc/song/123");
        assert_eq!(
            track.cover.as_deref(),
            Some("https://statcore.hitmcdn.com/cover.jpg")
        );
        assert_eq!(track.duration, 330);
    }
    #[test]
    #[ignore = "Checks live Hitmo search, song page and the first bytes of audio"]
    fn live_hitmo_search_and_audio() {
        use std::io::Read;
        let services = Services::new(String::new(), String::new(), String::new());
        let tracks = services.hitmotop_search("Massive Attack Teardrop").unwrap();
        assert!(!tracks.is_empty());
        let track = tracks
            .iter()
            .find(|t| t.artist == "Massive Attack" && t.title == "Teardrop")
            .unwrap();
        assert!(track.page_url.contains("/song/"));
        let stream = services.hitmotop_stream(track).unwrap();
        let res = request(services.http.get(&stream).header("Range", "bytes=0-8191")).unwrap();
        let content_type = res
            .headers()
            .get("content-type")
            .and_then(|s| s.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let mut bytes = vec![];
        res.take(8192).read_to_end(&mut bytes).unwrap();
        assert!(
            bytes.starts_with(b"ID3")
                || bytes
                    .windows(2)
                    .any(|b| b[0] == 0xff && (b[1] & 0xe0) == 0xe0)
        );
        println!("Hitmo: {} search results; song page resolved; {} audio bytes validated ({content_type})",tracks.len(),bytes.len());
    }
    #[test]
    fn youtube_extracts_song_id() {
        let mut tracks = vec![];
        parse_youtube(
            &j!({"musicResponsiveListItemRenderer":{"playlistItemData":{"videoId":"abcdef"},"flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Song"}]}}},{"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Artist"},{"text":"3:25"}]}}}]}}),
            &mut tracks,
        );
        assert_eq!(tracks[0].id, "abcdef");
        assert_eq!(tracks[0].duration, 205);
    }
}
