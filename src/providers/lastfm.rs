use super::*;
use crate::discovery::Sampler;

impl Services {
    pub(super) fn lastfm_call(&self, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        if self.lastfm.is_empty() {
            bail!("Добавьте API-ключ Last.fm в настройках");
        }
        let mut args = vec![
            ("method", method),
            ("api_key", self.lastfm.as_str()),
            ("format", "json"),
            ("autocorrect", "1"),
        ];
        args.extend_from_slice(params);
        let value = request_json(
            self.http
                .get("https://ws.audioscrobbler.com/2.0/")
                .query(&args),
        )?;
        if value.get("error").is_some() {
            bail!("Last.fm: {}", text(&value, "message"));
        }
        Ok(value)
    }
    pub fn recommendations(
        &self,
        seeds: &[Track],
        excluded: &HashSet<String>,
        discovery: f32,
        recent: &HashSet<String>,
    ) -> Result<Vec<Track>> {
        if seeds.is_empty() {
            bail!("Выберите исходный трек или импортируйте понравившиеся из Яндекса");
        }
        let mut sampler = Sampler::fresh();
        let mut tracks = Vec::new();
        let mut failure = None;
        for seed in seeds.iter().take(4) {
            match self.lastfm_call(
                "track.getSimilar",
                &[
                    ("artist", &seed.artist),
                    ("track", &seed.title),
                    ("limit", "60"),
                ],
            ) {
                Ok(value) => {
                    if let Some(items) = value
                        .pointer("/similartracks/track")
                        .and_then(Value::as_array)
                    {
                        for item in items {
                            if let Some(mut t) = parse_track(item) {
                                t.score = item
                                    .get("match")
                                    .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
                                    .unwrap_or(0.0);
                                t.reason = format!("Похожие на {} — {}", seed.artist, seed.title);
                                tracks.push(t);
                            }
                        }
                    }
                }
                Err(e) => failure = Some(e),
            }
        }
        // Sparse track graphs are expanded through related artists, never replaced with a global chart.
        if tracks
            .iter()
            .filter(|t| !excluded.contains(&t.identity()))
            .count()
            < 12
            || discovery >= 0.6
        {
            let value = match self.lastfm_call(
                "artist.getSimilar",
                &[("artist", &seeds[0].artist), ("limit", "12")],
            ) {
                Ok(value) => Some(value),
                Err(error) => {
                    failure = Some(error);
                    None
                }
            };
            if let Some(artists) = value
                .as_ref()
                .and_then(|v| v.pointer("/similarartists/artist"))
                .and_then(Value::as_array)
            {
                let mut artists = artists.clone();
                sampler.shuffle(&mut artists);
                for artist in artists.iter().take(4) {
                    let name = text(artist, "name");
                    if let Ok(top) = self
                        .lastfm_call("artist.getTopTracks", &[("artist", &name), ("limit", "8")])
                    {
                        if let Some(items) =
                            top.pointer("/toptracks/track").and_then(Value::as_array)
                        {
                            for item in items {
                                if let Some(mut t) = parse_track(item) {
                                    t.score = 0.2;
                                    t.reason = format!(
                                        "Открытия по {} — {}",
                                        seeds[0].artist, seeds[0].title
                                    );
                                    tracks.push(t);
                                }
                            }
                        }
                    }
                }
            }
        }
        if tracks.is_empty() {
            if let Some(e) = failure {
                return Err(e);
            }
        }
        let mut excluded = excluded.clone();
        excluded.extend(seeds.iter().map(Track::identity));
        let tracks = sampler.recommendations(tracks, &excluded, recent, discovery);
        Ok(tracks)
    }
}

fn parse_track(v: &Value) -> Option<Track> {
    let title = text(v, "name");
    let artist = v
        .get("artist")
        .map(|a| {
            if a.is_string() {
                a.as_str().unwrap_or_default().to_owned()
            } else {
                text(a, "name")
            }
        })
        .unwrap_or_default();
    if title.is_empty() || artist.is_empty() {
        return None;
    }
    let page = text(v, "url");
    let cover = v.get("image").and_then(Value::as_array).and_then(|a| {
        a.iter().rev().find_map(|i| {
            i.get("#text")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
    });
    Some(Track {
        id: format!("{artist}/{title}"),
        provider: Provider::LastFm,
        album: String::new(),
        title,
        artist,
        duration: 0,
        cover,
        page_url: page,
        edition: Edition::Unknown,
        reason: String::new(),
        score: 0.0,
    })
}
