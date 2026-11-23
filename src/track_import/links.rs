use super::{array, field, vk_html, Song, Source, FILE_LIMIT};
use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, redirect::Policy};
use scraper::{Html, Selector};
use serde_json::Value;
use std::{io::Read, time::Duration};
use url::Url;

const VK_HOSTS: &[&str] = &[
    "vk.com",
    "www.vk.com",
    "m.vk.com",
    "vk.ru",
    "m.vk.ru",
    "music.vk.com",
];
const SPOTIFY_HOSTS: &[&str] = &["open.spotify.com"];

pub(super) fn allowed(url: &Url, hosts: &[&str]) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| hosts.contains(&host))
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}

fn spotify_url(url: &Url) -> Result<Url> {
    let segments: Vec<_> = url.path().trim_end_matches('/').split('/').collect();
    let id = match segments.as_slice() {
        ["", "playlist", id] | ["", "embed", "playlist", id] => *id,
        ["", language, "playlist", id] if language.starts_with("intl-") => *id,
        _ => "",
    };
    anyhow::ensure!(
        allowed(url, SPOTIFY_HOSTS)
            && id.len() == 22
            && id.bytes().all(|c| c.is_ascii_alphanumeric()),
        "Вставьте ссылку на плейлист Spotify или скопируйте треки из браузера"
    );
    Ok(Url::parse(&format!(
        "https://open.spotify.com/embed/playlist/{id}"
    ))?)
}

fn vk_audio_id(value: &str) -> Option<&str> {
    let id = value
        .strip_prefix("audios")
        .or_else(|| value.strip_prefix("id"))?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

fn vk_playlist_id(value: &str) -> bool {
    let mut parts = value.split('_');
    let owner = parts.next().and_then(|id| id.parse::<i64>().ok());
    let playlist = parts.next().and_then(|id| id.parse::<u64>().ok());
    owner.is_some()
        && playlist.is_some()
        && parts
            .next()
            .is_none_or(|key| !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric()))
        && parts.next().is_none()
}

fn vk_url(url: &Url) -> Result<(Url, bool)> {
    anyhow::ensure!(
        allowed(url, VK_HOSTS),
        "Вставьте ссылку на страницу или плейлист VK"
    );
    let path = url.path().trim_matches('/');
    if let Some(id) = path
        .strip_prefix("music/playlist/")
        .filter(|id| vk_playlist_id(id))
    {
        return Ok((
            Url::parse(&format!("https://vk.com/music/playlist/{id}"))?,
            false,
        ));
    }
    if let Some(id) = url
        .query_pairs()
        .filter(|(k, _)| k == "z" || k == "act")
        .find_map(|(_, value)| {
            value
                .strip_prefix("audio_playlist-")
                .filter(|id| vk_playlist_id(id))
                .map(str::to_owned)
        })
    {
        return Ok((
            Url::parse(&format!("https://vk.com/music/playlist/{id}"))?,
            false,
        ));
    }
    if let Some(id) = vk_audio_id(path) {
        return Ok((Url::parse(&format!("https://vk.com/audios{id}"))?, false));
    }
    if path == "audio" {
        if let Some((_, id)) = url.query_pairs().find(|(key, value)| {
            key == "id" && !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
        }) {
            return Ok((Url::parse(&format!("https://vk.com/audios{id}"))?, false));
        }
        return Ok((Url::parse("https://vk.com/audio")?, false));
    }
    anyhow::ensure!(
        (2..=64).contains(&path.len())
            && path
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "Вставьте ссылку на страницу или плейлист VK"
    );
    Ok((Url::parse(&format!("https://vk.com/{path}"))?, true))
}

fn select_vk_audios(html: &str, profile: &Url) -> Option<Url> {
    let doc = Html::parse_document(html);
    let links = Selector::parse("a[href*='audios'], a[href*='/audio?']").unwrap();
    let audios: std::collections::HashSet<_> = doc
        .select(&links)
        .filter_map(|link| {
            let target = profile.join(link.attr("href")?).ok()?;
            if !allowed(&target, VK_HOSTS) {
                return None;
            }
            vk_audio_id(target.path().trim_matches('/'))
                .map(str::to_owned)
                .or_else(|| {
                    (target.path() == "/audio")
                        .then(|| {
                            target
                                .query_pairs()
                                .find(|(k, value)| {
                                    k == "id" && value.bytes().all(|b| b.is_ascii_digit())
                                })
                                .map(|(_, v)| v.into_owned())
                        })
                        .flatten()
                })
        })
        .collect();
    if audios.len() != 1 {
        return None;
    }
    let id = audios.into_iter().next()?;
    Url::parse(&format!("https://vk.com/audios{id}")).ok()
}

fn read(client: &Client, url: &Url) -> Result<String> {
    let response = client
        .get(url.clone())
        .send()
        .map_err(|e| e.without_url())?;
    anyhow::ensure!(response.status().is_success(), "Страница музыки недоступна");
    let mut html = String::new();
    response
        .take((FILE_LIMIT + 1) as u64)
        .read_to_string(&mut html)?;
    anyhow::ensure!(html.len() <= FILE_LIMIT, "Страница музыки слишком большая");
    Ok(html)
}

fn spotify_embed(contents: &str) -> Vec<Song> {
    let html = Html::parse_document(contents);
    let script = Selector::parse("script#__NEXT_DATA__").unwrap();
    let Some(value) = html
        .select(&script)
        .next()
        .and_then(|s| serde_json::from_str::<Value>(&s.text().collect::<String>()).ok())
    else {
        return Vec::new();
    };
    let data = &value["props"]["pageProps"]["state"]["data"]["entity"];
    let list = &data["trackList"];
    let tracks: Vec<_> = array(list)
        .filter(|item| !field(item, &["uri"]).starts_with("spotify:episode:"))
        .filter_map(|item| Song::new(field(item, &["title"]), field(item, &["subtitle"])))
        .collect();
    let count = ["totalCount", "trackCount"]
        .iter()
        .find_map(|key| data[*key].as_u64());
    if count.is_some_and(|count| list.as_array().is_none_or(|list| count > list.len() as u64))
        || (count.is_none() && list.as_array().is_some_and(|list| list.len() >= 100))
    {
        return Vec::new();
    }
    tracks
}

pub(super) fn load(source: Source, link: &str) -> Result<Vec<Song>> {
    let url = Url::parse(link).context("Вставьте полную ссылку на музыку")?;
    let (page, profile, hosts) = match source {
        Source::Vk => {
            let (page, profile) = vk_url(&url)?;
            (page, profile, VK_HOSTS)
        }
        Source::Spotify => (spotify_url(&url)?, false, SPOTIFY_HOSTS),
        Source::Yandex => bail!("Нужна ссылка Яндекс Музыки"),
    };
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() < 5 && allowed(attempt.url(), hosts) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?;
    let songs = (|| {
        let mut html = read(&client, &page)?;
        if profile {
            let audio =
                select_vk_audios(&html, &page).context("На странице нет открытого списка аудио")?;
            html = read(&client, &audio)?;
        }
        Ok::<_, anyhow::Error>(match source {
            Source::Vk => vk_html(&html),
            Source::Spotify => spotify_embed(&html),
            Source::Yandex => unreachable!(),
        })
    })()
    .unwrap_or_default();
    anyhow::ensure!(
        !songs.is_empty(),
        "{} скрывает список для этой ссылки. Скопируйте треки из браузера.",
        source.label()
    );
    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_music_links_without_following_external_hosts() {
        assert_eq!(
            vk_url(&Url::parse("https://m.vk.com/id42").unwrap())
                .unwrap()
                .0
                .as_str(),
            "https://vk.com/audios42"
        );
        assert_eq!(
            vk_url(&Url::parse("https://vk.com/audio?z=audio_playlist--42_12_key").unwrap())
                .unwrap()
                .0
                .as_str(),
            "https://vk.com/music/playlist/-42_12_key"
        );
        assert!(vk_url(&Url::parse("https://vk.com.example.org/id42").unwrap()).is_err());
        assert!(spotify_url(
            &Url::parse(
                "https://open.spotify.com/intl-en/playlist/37i9dQZF1DXcBWIGoYBM5M?si=share"
            )
            .unwrap()
        )
        .is_ok());
        let html = r#"<script id="__NEXT_DATA__">{"props":{"pageProps":{"state":{"data":{"entity":{"trackCount":1,"trackList":[{"title":"Jóga","subtitle":"Björk","uri":"spotify:track:1"}]}}}}}}</script>"#;
        assert_eq!(spotify_embed(html)[0].label(), "Björk — Jóga");
        assert!(spotify_embed(&html.replace("\"trackCount\":1", "\"trackCount\":5")).is_empty());
    }
}
