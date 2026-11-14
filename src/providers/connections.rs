use super::*;
use reqwest::Url;

pub const DEFAULT_HITMO: &str = "https://ru.hitmo-top.cc/";

pub fn hitmo_origin(value: &str) -> Result<String> {
    let value = value.trim();
    let url = Url::parse(&if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    })
    .context("Некорректный домен Hitmo")?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("Укажите только домен Hitmo, без пути, параметров и логина");
    }
    Ok(url.to_string())
}

impl Services {
    pub fn hitmo_page_url(&self, original: &str) -> Result<String> {
        let original = Url::parse(original).context("Некорректная страница Hitmo")?;
        let mut target = Url::parse(&self.hitmo_base)?;
        // Older catalogs may have saved a direct CDN file instead of a song
        // page. A mirror setting must not move that external audio file.
        let audio_file = [".mp3", ".m4a", ".aac", ".ogg"]
            .iter()
            .any(|extension| original.path().to_ascii_lowercase().ends_with(extension));
        if audio_file
            && original.host_str() != target.host_str()
            && original.host_str() != Url::parse(DEFAULT_HITMO)?.host_str()
        {
            return Ok(original.to_string());
        }
        target.set_path(original.path());
        target.set_query(original.query());
        Ok(target.to_string())
    }

    pub fn check_connection(&self, provider: Provider, value: &str) -> Result<String> {
        let mut service = self.clone();
        service.http = Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(8))
            .user_agent("WeDex connection check")
            .build()?;
        let value = value.trim();
        match provider {
            Provider::Yandex => {
                service.yandex = value.into();
                let account = service.ym("account/status", &[])?;
                if id(&account["account"]["uid"]).is_empty() {
                    bail!("Яндекс не подтвердил аккаунт");
                }
                let name = text(&account["account"], "displayName");
                Ok(if name.is_empty() {
                    "Аккаунт подтверждён".into()
                } else {
                    format!("Аккаунт: {name}")
                })
            }
            Provider::LastFm => {
                service.lastfm = value.into();
                let response = service.lastfm_call("chart.getTopTracks", &[("limit", "1")])?;
                if response.get("tracks").is_none() {
                    bail!("Last.fm не подтвердил доступ к API");
                }
                Ok("API-ключ принят · рекомендации доступны".into())
            }
            Provider::SoundCloud => {
                if value.is_empty() {
                    bail!("Добавьте OAuth-токен SoundCloud");
                }
                service.soundcloud = value.into();
                let _ = service.soundcloud_search("music")?;
                Ok("Токен принят · поиск доступен".into())
            }
            Provider::Hitmotop => {
                service.hitmo_base = hitmo_origin(value)?;
                let _ = service.hitmotop_search("music")?;
                Ok("Домен отвечает".into())
            }
            Provider::YouTube => {
                if service.youtube_search("music")?.is_empty() {
                    bail!("YouTube Music не вернул результаты");
                }
                Ok("Поиск доступен без API-ключа".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Checks the public Hitmo catalogue without credentials or playback"]
    fn live_discovered_hitmo_catalogue() {
        let mut services = Services::new(String::new(), String::new(), String::new());
        services.hitmo_base = "https://ru.hitmox.net/".into();
        let tracks = services.hitmotop_search("music").unwrap();
        assert!(!tracks.is_empty());
        assert!(tracks
            .iter()
            .all(|t| t.provider == Provider::Hitmotop && !t.title.is_empty()));
        println!(
            "Verified public Hitmo catalogue: {} parsed tracks",
            tracks.len()
        );
    }
    #[test]
    #[ignore = "Checks configured Yandex and Last.fm credentials without saving or playing audio"]
    fn live_configured_connections() {
        let service = Services::new(String::new(), String::new(), String::new());
        for (provider, key) in [(Provider::Yandex, "yandex"), (Provider::LastFm, "lastfm")] {
            let secret = crate::store::secret(key);
            assert!(!secret.is_empty(), "a configured credential is required");
            assert!(!service
                .check_connection(provider, &secret)
                .unwrap()
                .is_empty());
            println!("{:?}: validation succeeded", provider);
        }
    }
    #[test]
    fn search_and_saved_song_resolution_use_the_new_mirror_with_the_same_routes() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            for expected in ["/search?q=music", "/song/42?version=1"] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = [0; 8192];
                let count = socket.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                assert!(
                    request.starts_with(&format!("GET {expected} HTTP/1.1")),
                    "wrong mirror route"
                );
                let body = if expected.starts_with("/search") {
                    r#"<li class="tracks__item"><a class="track__info-l" href="/song/42?version=1"><div class="track__title">Song</div><div class="track__desc">Artist</div></a></li>"#
                } else {
                    r#"<a class="track__download-btn" href="/get/song.mp3"></a>"#
                };
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let mut service = Services::new(String::new(), String::new(), String::new());
        service.hitmo_base = hitmo_origin(&format!("http://{address}")).unwrap();
        let found = service.hitmotop_search("music").unwrap();
        assert_eq!(found.len(), 1);
        let mut old = found[0].clone();
        old.page_url = "https://old.example/song/42?version=1".into();
        assert_eq!(
            service.hitmotop_stream(&old).unwrap(),
            format!("http://{address}/get/song.mp3")
        );
        worker.join().unwrap();
    }
    #[test]
    fn mirror_changes_only_origin_and_preserves_song_paths_and_queries() {
        let mut service = Services::new(String::new(), String::new(), String::new());
        service.hitmo_base = hitmo_origin("mirror.example").unwrap();
        assert_eq!(
            service
                .hitmo_page_url("https://old.example/song/123?download=1")
                .unwrap(),
            "https://mirror.example/song/123?download=1"
        );
        assert_eq!(
            service
                .hitmo_page_url("https://cdn.example/file.mp3?signature=fixture")
                .unwrap(),
            "https://cdn.example/file.mp3?signature=fixture"
        );
        assert_eq!(
            service
                .hitmo_page_url("https://ru.hitmo-top.cc/get/file.mp3")
                .unwrap(),
            "https://mirror.example/get/file.mp3"
        );
        assert_eq!(
            hitmo_origin("https://mirror.example/").unwrap(),
            "https://mirror.example/"
        );
        for invalid in [
            "",
            "https://mirror.example/search",
            "https://user:secret@mirror.example",
            "https://mirror.example/?q=a",
            "file:///music",
            "https://mirror.example/#x",
        ] {
            assert!(hitmo_origin(invalid).is_err(), "invalid origin accepted");
        }
    }
}
