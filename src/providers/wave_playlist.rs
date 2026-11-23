use super::{yandex::parse_track, *};
use crate::model::normalize;
use serde::Serialize;
use serde_json::json;
use std::{collections::HashMap, sync::Mutex};

const TITLE: &str = "wedex-yandex";
const DESCRIPTION: &str = "WeDex · Понравившиеся для Моей волны";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogTrack {
    id: u64,
    album_id: u64,
}

#[derive(Default)]
pub(super) struct Cache {
    uid: String,
    matches: HashMap<String, Option<CatalogTrack>>,
    pending_kind: Option<u64>,
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn catalog_track(value: &Value) -> Option<CatalogTrack> {
    if value["available"] == false && value["availableForPremiumUsers"] != true {
        return None;
    }
    Some(CatalogTrack {
        id: number(&value["id"])?,
        album_id: number(&value["albums"][0]["id"]).or_else(|| number(&value["albumId"]))?,
    })
}

fn match_catalog(track: &Track, value: &Value) -> Option<CatalogTrack> {
    let title = normalize(&track.title);
    let artist = normalize(&track.artist);
    value
        .pointer("/tracks/results")?
        .as_array()?
        .iter()
        .find_map(|value| {
            let candidate = parse_track(value)?;
            (normalize(&candidate.title) == title
                && normalize(&candidate.artist) == artist
                && match_score(track, &candidate) >= 0.94)
                .then(|| catalog_track(value))
                .flatten()
        })
}

fn edits(current: &[Option<u64>], desired: &[CatalogTrack]) -> Vec<Value> {
    let wanted: HashSet<_> = desired.iter().map(|t| t.id).collect();
    let mut kept = HashSet::new();
    let mut removals = Vec::<std::ops::Range<usize>>::new();
    for (index, id) in current.iter().enumerate() {
        if id.is_some_and(|id| wanted.contains(&id) && kept.insert(id)) {
            continue;
        }
        match removals.last_mut() {
            Some(last) if last.end == index => last.end += 1,
            _ => removals.push(index..index + 1),
        }
    }
    let mut diff: Vec<_> = removals
        .into_iter()
        .rev()
        .map(|r| json!({"op":"delete","from":r.start,"to":r.end}))
        .collect();
    let added: Vec<_> = desired
        .iter()
        .filter(|track| !kept.contains(&track.id))
        .collect();
    for batch in added.rchunks(100) {
        diff.push(json!({"op":"insert","at":0,"tracks":batch}));
    }
    diff
}

impl Services {
    fn resolve_wave_favorites(
        &self,
        cache: &mut Cache,
        favorites: &[Track],
    ) -> Result<Vec<CatalogTrack>> {
        let keys: Vec<_> = favorites
            .iter()
            .map(|t| format!("{}:{}", t.key(), t.identity()))
            .collect();
        let active: HashSet<_> = keys.iter().collect();
        cache.matches.retain(|key, _| active.contains(key));
        let mut catalog_ids = Vec::new();
        let mut searches = Vec::new();
        for (key, track) in keys.iter().zip(favorites) {
            if cache.matches.contains_key(key) {
                continue;
            }
            if track.provider == Provider::Yandex && track.id.bytes().all(|b| b.is_ascii_digit()) {
                catalog_ids.push((key, track.id.as_str()));
            } else {
                searches.push((key, track));
            }
        }
        for batch in catalog_ids.chunks(100) {
            let ids = batch
                .iter()
                .map(|(_, id)| *id)
                .collect::<Vec<_>>()
                .join(",");
            let response = self.ym("tracks", &[("track-ids", &ids)])?;
            let tracks: HashMap<_, _> = response
                .as_array()
                .context("Нет метаданных Яндекса")?
                .iter()
                .filter_map(catalog_track)
                .map(|t| (t.id, t))
                .collect();
            for (key, id) in batch {
                cache.matches.insert(
                    (*key).clone(),
                    id.parse().ok().and_then(|id| tracks.get(&id).copied()),
                );
            }
        }
        let pending = Mutex::new(searches.into_iter());
        let mut results = Vec::new();
        std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        let mut results = Vec::new();
                        loop {
                            let Some((key, track)) = pending.lock().unwrap().next() else {
                                break;
                            };
                            results.push((
                                key,
                                self.yandex_search_tracks(&track.query())
                                    .map(|value| match_catalog(track, &value)),
                            ));
                            if results.last().is_some_and(|(_, result)| result.is_err()) {
                                break;
                            }
                        }
                        results
                    })
                })
                .collect();
            for job in jobs {
                results.extend(job.join().unwrap());
            }
        });
        let mut failure = None;
        for (key, result) in results {
            match result {
                Ok(track) => {
                    cache.matches.insert(key.clone(), track);
                }
                Err(error) => {
                    failure = Some(error);
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        let mut seen = HashSet::new();
        let mut tracks: Vec<_> = keys
            .iter()
            .filter_map(|key| cache.matches.get(key).copied().flatten())
            .filter(|track| seen.insert(track.id))
            .collect();
        tracks.truncate(10_000);
        Ok(tracks)
    }

    fn managed_playlist(&self, cache: &mut Cache, create: bool) -> Result<Option<u64>> {
        let base = format!("users/{}/playlists", cache.uid);
        let playlists = self.ym(&format!("{base}/list"), &[])?;
        for candidate in playlists
            .as_array()
            .context("Яндекс не вернул список плейлистов")?
        {
            if candidate["title"] != TITLE {
                continue;
            }
            let Some(kind) = number(&candidate["kind"]) else {
                continue;
            };
            let playlist = self.ym(&format!("{base}/{kind}"), &[("rich-tracks", "false")])?;
            if playlist["visibility"] != "private" {
                continue;
            }
            if playlist["description"] == DESCRIPTION {
                cache.pending_kind = None;
                return Ok(Some(kind));
            }
            if cache.pending_kind == Some(kind)
                && playlist["description"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
            {
                self.yandex_edit_playlist(
                    &format!("{base}/{kind}/description"),
                    &[("value", DESCRIPTION)],
                )?;
                cache.pending_kind = None;
                return Ok(Some(kind));
            }
        }
        if !create {
            return Ok(None);
        }
        let new = self.yandex_edit_playlist(
            &format!("{base}/create"),
            &[("title", TITLE), ("visibility", "private")],
        )?;
        let kind = number(&new["kind"]).context("Яндекс не вернул ID нового плейлиста")?;
        cache.pending_kind = Some(kind);
        self.yandex_edit_playlist(
            &format!("{base}/{kind}/description"),
            &[("value", DESCRIPTION)],
        )?;
        cache.pending_kind = None;
        Ok(Some(kind))
    }

    pub(super) fn sync_wave_playlist(&self, favorites: &[Track]) -> Result<Option<String>> {
        let mut cache = self.wave_playlist.lock().unwrap();
        let (uid, _) = self.yandex_account()?;
        if cache.uid != uid {
            *cache = Cache {
                uid,
                ..Cache::default()
            };
        }
        let tracks = self.resolve_wave_favorites(&mut cache, favorites)?;
        let Some(kind) = self.managed_playlist(&mut cache, !tracks.is_empty())? else {
            return Ok(None);
        };
        let path = format!("users/{}/playlists/{kind}", cache.uid);
        let load = || self.ym(&path, &[("rich-tracks", "false")]);
        let mut last_error = None;
        for _ in 0..2 {
            let playlist = load()?;
            anyhow::ensure!(
                playlist["title"] == TITLE
                    && playlist["description"] == DESCRIPTION
                    && playlist["visibility"] == "private",
                "Служебный плейлист Яндекса был изменён"
            );
            let count = number(&playlist["trackCount"]);
            let items = playlist["tracks"].as_array().map(Vec::as_slice);
            let current: Vec<_> = items
                .or_else(|| (count == Some(0)).then_some(&[]))
                .context("Яндекс не вернул треки плейлиста")?
                .iter()
                .map(|item| number(&item["id"]).or_else(|| number(&item["track"]["id"])))
                .collect();
            anyhow::ensure!(
                count.is_none_or(|count| count as usize == current.len()),
                "Яндекс вернул неполный список треков плейлиста"
            );
            let diff = edits(&current, &tracks);
            if diff.is_empty() {
                return Ok((!tracks.is_empty()).then(|| format!("playlist:{}_{kind}", cache.uid)));
            }
            let revision =
                number(&playlist["revision"]).context("Яндекс не вернул ревизию плейлиста")?;
            match self.yandex_edit_playlist(
                &format!("{path}/change"),
                &[
                    ("kind", &kind.to_string()),
                    ("revision", &revision.to_string()),
                    ("diff", &serde_json::to_string(&diff)?),
                ],
            ) {
                Ok(_) => {
                    return Ok(
                        (!tracks.is_empty()).then(|| format!("playlist:{}_{kind}", cache.uid))
                    )
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.context("Не удалось обновить плейлист Яндекса")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
    };

    #[test]
    fn update_keeps_matching_songs_and_removes_duplicates_in_reverse_index_order() {
        let track = |id| CatalogTrack {
            id,
            album_id: id + 20,
        };
        assert_eq!(
            edits(
                &[Some(1), Some(2), Some(1), Some(3), Some(4), Some(5)],
                &[track(1), track(4), track(8)]
            ),
            vec![
                json!({"op":"delete","from":5,"to":6}),
                json!({"op":"delete","from":1,"to":4}),
                json!({"op":"insert","at":0,"tracks":[{"id":8,"albumId":28}]}),
            ]
        );
        assert!(edits(&[Some(1), Some(4)], &[track(4), track(1)]).is_empty());
    }

    #[test]
    fn starts_wave_from_the_private_playlist_after_writing_the_favorites() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut services = Services::new("fixture-token".into(), String::new(), String::new());
        services.yandex_test_origin = Some(format!("http://{}", listener.local_addr().unwrap()));
        let fixture = std::thread::spawn(move || {
            let steps = [
                ("/account/status", json!({"account":{"uid":17}})),
                ("/tracks", json!([{"id":21,"albums":[{"id":75}]}])),
                ("/users/17/playlists/list", json!([])),
                ("/users/17/playlists/create", json!({"kind":1001})),
                ("/users/17/playlists/1001/description", json!({})),
                (
                    "/users/17/playlists/1001",
                    json!({"revision":4,"title":TITLE,"description":DESCRIPTION,"visibility":"private","trackCount":0}),
                ),
                ("/users/17/playlists/1001/change", json!({"revision":5})),
                (
                    "/rotor/session/new",
                    json!({"radioSessionId":"test","batchId":"one","sequence":[{"track":{"id":22,"title":"Other Song","artists":[{"name":"Artist"}]}}]}),
                ),
            ];
            for (path, result) in steps {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(
                    line.split_whitespace()
                        .nth(1)
                        .unwrap()
                        .split('?')
                        .next()
                        .unwrap(),
                    path
                );
                let mut size = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        size = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                let form: HashMap<_, _> = url::form_urlencoded::parse(&body).into_owned().collect();
                match path {
                    "/users/17/playlists/create" => {
                        assert_eq!(form["title"], TITLE);
                        assert_eq!(form["visibility"], "private");
                    }
                    "/users/17/playlists/1001/description" => {
                        assert_eq!(form["value"], DESCRIPTION)
                    }
                    "/users/17/playlists/1001/change" => {
                        assert_eq!(form["revision"], "4");
                        assert_eq!(
                            serde_json::from_str::<Value>(&form["diff"]).unwrap(),
                            json!([{"op":"insert","at":0,"tracks":[{"id":21,"albumId":75}]}])
                        );
                    }
                    "/rotor/session/new" => {
                        let body: Value = serde_json::from_slice(&body).unwrap();
                        assert_eq!(body["seeds"], json!(["playlist:17_1001"]));
                    }
                    _ => {}
                }
                let body = json!({"result":result}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let mut favorite = crate::model::tests::track("Song", "Artist", 200);
        favorite.provider = Provider::Yandex;
        favorite.id = "21".into();
        let wave = services.wave(None, None, &[favorite], 0.4, &[]).unwrap();
        assert!(wave.session.basis() == super::super::wave::Basis::Favorites);
        assert_eq!(wave.tracks[0].id, "22");
        fixture.join().unwrap();
    }
}
