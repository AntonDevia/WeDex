pub mod connections;
mod external;
pub mod lastfm;
pub mod yandex;

use crate::model::{match_score, Artist, Edition, Provider, Track};
use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use serde_json::Value;
use std::{collections::HashSet, time::Duration};

#[derive(Clone)]
pub struct Services {
    pub http: Client,
    pub yandex: String,
    pub lastfm: String,
    pub soundcloud: String,
    pub hitmo_base: String,
}

#[cfg(test)]
mod live {
    use super::*;
    #[test]
    fn fast_search_batches_are_published_while_another_source_is_still_waiting() {
        let (release, wait) = std::sync::mpsc::channel();
        let (publish, batches) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let jobs: Vec<SearchJob<'_>> = vec![
                (
                    "Slow artists",
                    Box::new(move || {
                        wait.recv_timeout(Duration::from_secs(2)).unwrap();
                        SearchResult {
                            artists: vec![Artist {
                                id: "artist".into(),
                                name: "Artist".into(),
                                cover: None,
                                tracks: None,
                                listeners: None,
                            }],
                            ..Default::default()
                        }
                    }),
                ),
                (
                    "Fast tracks",
                    Box::new(|| SearchResult {
                        tracks: vec![crate::model::tests::track("Song", "Artist", 100)],
                        ..Default::default()
                    }),
                ),
            ];
            search_jobs(jobs, &|batch| {
                publish.send(batch).unwrap();
            });
        });
        let first = batches.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first.tracks.len(), 1);
        assert!(first.artists.is_empty());
        release.send(()).unwrap();
        assert_eq!(
            batches
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .artists
                .len(),
            1
        );
        worker.join().unwrap();
    }
    #[test]
    #[ignore = "Measures live catalog latency with configured accounts without playing audio"]
    fn live_search_latency() {
        let services = Services::new(
            crate::store::secret("yandex"),
            String::new(),
            crate::store::secret("soundcloud"),
        );
        let started = std::time::Instant::now();
        let arrivals = std::sync::Mutex::new(Vec::new());
        services.search_progress("Kambulat", true, |batch| {
            let label = batch.tracks.first().map(|t| t.provider.label()).unwrap_or(
                if !batch.artists.is_empty() {
                    "Yandex artists"
                } else {
                    "Empty/error source"
                },
            );
            println!(
                "{label}: {} ms, {} tracks, {} artists, {} errors",
                started.elapsed().as_millis(),
                batch.tracks.len(),
                batch.artists.len(),
                batch.notices.len()
            );
            arrivals
                .lock()
                .unwrap()
                .push((batch.tracks.len() + batch.artists.len(), started.elapsed()));
        });
        let arrivals = arrivals.into_inner().unwrap();
        assert!(arrivals.iter().any(|(count, _)| *count > 0));
        println!(
            "First useful batch: {} ms; all sources finished: {} ms",
            arrivals
                .iter()
                .filter(|(count, _)| *count > 0)
                .map(|(_, t)| t.as_millis())
                .min()
                .unwrap(),
            started.elapsed().as_millis()
        );
    }
    #[test]
    fn mixed_sources_do_not_bury_hitmo_under_the_yandex_catalog() {
        let mut tracks: Vec<_> = (0..70)
            .map(|i| crate::model::tests::track(&format!("Song {i}"), "Artist", 200))
            .collect();
        for provider in [Provider::Hitmotop, Provider::YouTube] {
            let mut track = crate::model::tests::track("Match", "Artist", 200);
            track.provider = provider;
            tracks.push(track);
        }
        let ordered = interleave_sources(tracks);
        assert_eq!(ordered.len(), 72);
        assert_eq!(ordered[0].provider, Provider::Yandex);
        assert_eq!(ordered[1].provider, Provider::Hitmotop);
        assert_eq!(ordered[2].provider, Provider::YouTube);
        assert_eq!(ordered[3].title, "Song 1");
        assert_eq!(ordered.last().unwrap().title, "Song 69");
    }
    #[test]
    #[ignore = "Calls the live Last.fm service with your configured API key"]
    fn lastfm_recommendations() {
        let s = Services::new(String::new(), crate::store::secret("lastfm"), String::new());
        let seed = crate::model::tests::track("Teardrop", "Massive Attack", 330);
        let tracks = s
            .recommendations(
                std::slice::from_ref(&seed),
                &HashSet::new(),
                0.5,
                &HashSet::new(),
            )
            .unwrap();
        assert!(!tracks.is_empty());
        let recent = tracks.iter().map(Track::identity).collect();
        let refreshed = s
            .recommendations(&[seed], &HashSet::new(), 0.5, &recent)
            .unwrap();
        assert!(!refreshed.is_empty());
        if tracks.len() >= 8 {
            assert_ne!(
                tracks.iter().map(Track::identity).collect::<Vec<_>>(),
                refreshed.iter().map(Track::identity).collect::<Vec<_>>()
            );
        }
        println!(
            "Refresh: {} tracks, order differs: {}",
            refreshed.len(),
            tracks.iter().map(Track::identity).collect::<Vec<_>>()
                != refreshed.iter().map(Track::identity).collect::<Vec<_>>()
        );
        println!(
            "Last.fm: {} recommendations, first artist: {}",
            tracks.len(),
            tracks[0].artist
        );
    }
    #[test]
    #[ignore = "Reports availability of external services on this network"]
    fn public_catalogs() {
        let s = Services::new(String::new(), String::new(), String::new());
        for (name, result) in [
            ("Hitmo", s.hitmotop_search("Massive Attack Teardrop")),
            ("YouTube Music", s.youtube_search("Massive Attack Teardrop")),
        ] {
            match result {
                Ok(tracks) => println!("{name}: {} results", tracks.len()),
                Err(e) => println!("{name}: {e}"),
            }
        }
    }
}

#[derive(Default)]
pub struct SearchResult {
    pub tracks: Vec<Track>,
    pub artists: Vec<Artist>,
    pub notices: Vec<String>,
}

impl SearchResult {
    pub fn merge(&mut self, batch: SearchResult) {
        let mut tracks: HashSet<_> = self.tracks.iter().map(Track::key).collect();
        self.tracks
            .extend(batch.tracks.into_iter().filter(|t| tracks.insert(t.key())));
        self.tracks = interleave_sources(std::mem::take(&mut self.tracks));
        let mut artists: HashSet<_> = self.artists.iter().map(|a| a.id.clone()).collect();
        self.artists.extend(
            batch
                .artists
                .into_iter()
                .filter(|a| artists.insert(a.id.clone())),
        );
        self.notices.extend(batch.notices);
    }
}

type SearchJob<'a> = (&'static str, Box<dyn FnOnce() -> SearchResult + Send + 'a>);

fn search_jobs(jobs: Vec<SearchJob<'_>>, publish: &(impl Fn(SearchResult) + Sync)) {
    std::thread::scope(|scope| {
        let jobs: Vec<_> = jobs
            .into_iter()
            .map(|(name, work)| (name, scope.spawn(move || publish(work()))))
            .collect();
        for (name, job) in jobs {
            if job.join().is_err() {
                publish(SearchResult {
                    notices: vec![format!("{name}: ошибка адаптера")],
                    ..Default::default()
                });
            }
        }
    });
}

fn track_result(source: &str, result: Result<Vec<Track>>) -> SearchResult {
    match result {
        Ok(tracks) => SearchResult {
            tracks,
            ..Default::default()
        },
        Err(error) => SearchResult {
            notices: vec![format!("{source}: {error}")],
            ..Default::default()
        },
    }
}

/// Preserve each catalog's relevance, while exposing other sources immediately.
fn interleave_sources(tracks: Vec<Track>) -> Vec<Track> {
    let mut groups: Vec<_> = [
        Provider::Yandex,
        Provider::Hitmotop,
        Provider::SoundCloud,
        Provider::YouTube,
        Provider::LastFm,
    ]
    .into_iter()
    .map(|provider| {
        tracks
            .iter()
            .filter(|t| t.provider == provider)
            .cloned()
            .collect::<std::collections::VecDeque<_>>()
    })
    .collect();
    let mut ordered = Vec::with_capacity(tracks.len());
    loop {
        let before = ordered.len();
        for group in &mut groups {
            if let Some(track) = group.pop_front() {
                ordered.push(track);
            }
        }
        if ordered.len() == before {
            break;
        }
    }
    ordered
}

pub fn response(res: reqwest::Result<Response>) -> Result<Response> {
    // Request URLs may contain credentials. Do not include them in UI errors/logs.
    let res = res.map_err(|e| e.without_url())?;
    let status = res.status();
    if !status.is_success() {
        bail!("Сервис ответил HTTP {status}");
    }
    Ok(res)
}

pub fn json(res: reqwest::Result<Response>) -> Result<Value> {
    Ok(response(res)?.json().map_err(|e| e.without_url())?)
}
/// Catalog reads are safe to repeat after a connection reset. Stream addresses
/// are still renewed by the audio preparation layer when access is rejected.
pub fn request(builder: reqwest::blocking::RequestBuilder) -> Result<Response> {
    response(Ok(request_raw(builder)?))
}
pub(crate) fn request_raw(builder: reqwest::blocking::RequestBuilder) -> Result<Response> {
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(200 * attempt));
        }
        let result = builder
            .try_clone()
            .context("Запрос нельзя повторить")?
            .send();
        match result {
            Ok(res)
                if res.status().is_server_error()
                    || res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS =>
            {
                last = Some(anyhow::anyhow!("Сервис ответил HTTP {}", res.status()))
            }
            Ok(res) => return Ok(res),
            Err(error) => last = Some(anyhow::Error::from(error.without_url())),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("Сервис не ответил")))
}
pub fn request_json(builder: reqwest::blocking::RequestBuilder) -> Result<Value> {
    for attempt in 0..2 {
        let res = request(builder.try_clone().context("Запрос нельзя повторить")?)?;
        match res.json() {
            Ok(value) => return Ok(value),
            Err(error) if attempt == 0 && (error.is_body() || error.is_timeout()) => {
                std::thread::sleep(Duration::from_millis(200))
            }
            Err(error) => return Err(error.without_url().into()),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn catalog_read_recovers_from_connection_reset() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0_u8; 2048];
                assert!(socket.read(&mut request).unwrap() > 0);
                if attempt == 1 {
                    let body = r#"{"ok":true}"#;
                    write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                }
            }
        });
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        assert_eq!(
            request_json(client.get(format!("http://{addr}/search"))).unwrap()["ok"],
            true
        );
        server.join().unwrap();
    }
    #[test]
    fn access_denied_is_returned_without_catalog_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            assert!(socket.read(&mut request).unwrap() > 0);
            socket
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let error = request_json(Client::new().get(format!("http://{addr}/search"))).unwrap_err();
        assert!(error.to_string().contains("403"));
        server.join().unwrap();
    }
}
pub fn text(v: &Value, name: &str) -> String {
    v.get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
pub fn id(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.as_u64().map(|n| n.to_string()).unwrap_or_default())
}

impl Services {
    pub fn new(yandex: String, lastfm: String, soundcloud: String) -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(22))
            .user_agent("WeDex/0.1 (personal desktop music client)")
            .build()
            .expect("HTTP client");
        Self {
            http,
            yandex,
            lastfm,
            soundcloud,
            hitmo_base: connections::DEFAULT_HITMO.into(),
        }
    }
    pub fn search_progress(&self, query: &str, all: bool, publish: impl Fn(SearchResult) + Sync) {
        self.search_catalog(query, all, true, &publish);
    }
    fn search_catalog(
        &self,
        query: &str,
        all: bool,
        artists: bool,
        publish: &(impl Fn(SearchResult) + Sync),
    ) {
        let yandex_has_tracks = std::sync::atomic::AtomicBool::new(false);
        let mut jobs: Vec<SearchJob<'_>> = Vec::new();
        if !self.yandex.is_empty() {
            jobs.push((
                "Yandex",
                Box::new(|| {
                    let result = track_result("Yandex", self.yandex_search(query));
                    yandex_has_tracks.store(
                        !result.tracks.is_empty(),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    result
                }),
            ));
            if artists {
                jobs.push((
                    "Yandex artists",
                    Box::new(|| match self.yandex_search_artists(query) {
                        Ok(artists) => SearchResult {
                            artists,
                            ..Default::default()
                        },
                        Err(e) => SearchResult {
                            notices: vec![format!("Yandex artists: {e}")],
                            ..Default::default()
                        },
                    }),
                ));
            }
        }
        let external = || -> Vec<SearchJob<'_>> {
            let mut jobs: Vec<SearchJob<'_>> = vec![
                (
                    "Hitmo",
                    Box::new(|| track_result("Hitmo", self.hitmotop_search(query))),
                ),
                (
                    "YouTube",
                    Box::new(|| track_result("YouTube Music", self.youtube_search(query))),
                ),
            ];
            if !self.soundcloud.is_empty() {
                jobs.push((
                    "SoundCloud",
                    Box::new(|| track_result("SoundCloud", self.soundcloud_search(query))),
                ));
            }
            jobs
        };
        let external_started = all || self.yandex.is_empty();
        if external_started {
            jobs.extend(external());
        }
        search_jobs(jobs, publish);
        if !external_started && !yandex_has_tracks.load(std::sync::atomic::Ordering::Relaxed) {
            search_jobs(external(), publish);
        }
    }
    pub fn alternatives(&self, seed: &Track) -> SearchResult {
        let result = std::sync::Mutex::new(SearchResult::default());
        self.search_catalog(&seed.query(), true, false, &|batch| {
            result.lock().unwrap().merge(batch)
        });
        let mut result = result.into_inner().unwrap();
        for t in &mut result.tracks {
            t.score = match_score(seed, t);
        }
        result
            .tracks
            .retain(|t| t.key() != seed.key() && t.score >= 0.82);
        result.tracks.sort_by(|a, b| {
            let rank = |t: &Track| match t.edition {
                Edition::ConfirmedOriginal => 2,
                Edition::Explicit => 1,
                Edition::Unknown => 0,
                Edition::Clean => -1,
            };
            rank(b)
                .cmp(&rank(a))
                .then_with(|| b.score.total_cmp(&a.score))
        });
        result
    }
    pub fn resolve(&self, track: &Track) -> Result<String> {
        match track.provider {
            Provider::Yandex => self.yandex_stream(track),
            Provider::Hitmotop => self.hitmotop_stream(track),
            Provider::SoundCloud => self.soundcloud_stream(track),
            Provider::YouTube => self.youtube_stream(track),
            Provider::LastFm => {
                bail!("Last.fm предоставляет рекомендации, аудио нужно найти в каталоге")
            }
        }
    }
    pub fn resolve_recommendation(&self, seed: &Track, prefer_original: bool) -> Result<Track> {
        let choose = |tracks: Vec<Track>| {
            tracks
                .into_iter()
                .filter_map(|t| {
                    let s = match_score(seed, &t);
                    (s >= 0.86).then_some((t, s))
                })
                .max_by(|(a, sa), (b, sb)| {
                    let explicit = |t: &Track| {
                        if prefer_original {
                            match t.edition {
                                Edition::Explicit | Edition::ConfirmedOriginal => 0.03,
                                Edition::Clean => -0.1,
                                _ => 0.0,
                            }
                        } else {
                            0.0
                        }
                    };
                    (sa + explicit(a)).total_cmp(&(sb + explicit(b)))
                })
                .map(|(mut t, _)| {
                    t.reason = seed.reason.clone();
                    t
                })
        };
        let mut attempts = Vec::new();
        if !self.yandex.is_empty() {
            match self.yandex_search(&seed.query()) {
                Ok(tracks) => {
                    let count = tracks.len();
                    if let Some(t) = choose(tracks) {
                        return Ok(t);
                    }
                    attempts.push(format!("Яндекс: {count} записей, точного совпадения нет"));
                }
                Err(e) => attempts.push(format!("Яндекс: {e:#}")),
            }
        }
        for provider in [Provider::Hitmotop, Provider::SoundCloud, Provider::YouTube] {
            if provider == Provider::SoundCloud && self.soundcloud.is_empty() {
                continue;
            }
            let result = match provider {
                Provider::Hitmotop => self.hitmotop_search(&seed.query()),
                Provider::SoundCloud => self.soundcloud_search(&seed.query()),
                _ => self.youtube_search(&seed.query()),
            };
            match result {
                Ok(tracks) => {
                    let count = tracks.len();
                    if let Some(t) = choose(tracks) {
                        return Ok(t);
                    }
                    attempts.push(format!(
                        "{}: {count} записей, точного совпадения нет",
                        provider.label()
                    ));
                }
                Err(e) => attempts.push(format!("{}: {e:#}", provider.label())),
            }
        }
        bail!(
            "Не найдена точная запись для {} — {}. {}",
            seed.artist,
            seed.title,
            attempts.join("; ")
        )
    }
}
