use crate::{
    model::{normalize, Track},
    providers::{request, request_raw, response, Services},
};
use anyhow::Result;
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct Line {
    pub time: f32,
    pub text: String,
}
#[derive(Clone, Debug, Default)]
pub struct Lyrics {
    pub plain: String,
    pub lines: Vec<Line>,
    pub instrumental: bool,
}
impl Lyrics {
    pub fn active(&self, seconds: f32) -> Option<usize> {
        self.lines
            .partition_point(|line| line.time <= seconds)
            .checked_sub(1)
    }
}
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out = vec![];
    let mut offset = 0.;
    for line in text.lines() {
        if let Some(value) = line
            .strip_prefix("[offset:")
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| s.parse::<f32>().ok())
        {
            offset = value / 1000.;
        }
    }
    for line in text.lines() {
        let mut rest = line;
        let mut times = vec![];
        while let Some(tag) = rest.strip_prefix('[').and_then(|s| s.split_once(']')) {
            rest = tag.1;
            if let Some((minutes, seconds)) = tag.0.split_once(':') {
                if let (Ok(m), Ok(s)) = (minutes.parse::<f32>(), seconds.parse::<f32>()) {
                    if m.is_finite() && m >= 0. && (0. ..60.).contains(&s) {
                        times.push((m * 60. + s + offset).max(0.));
                    }
                }
            }
        }
        for time in times {
            out.push(Line {
                time,
                text: rest.trim().to_owned(),
            });
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}
fn decode(value: &Value) -> Lyrics {
    let plain = value["plainLyrics"].as_str().unwrap_or_default().to_owned();
    let lines = parse_lrc(value["syncedLyrics"].as_str().unwrap_or_default());
    Lyrics {
        plain,
        lines,
        instrumental: value["instrumental"].as_bool().unwrap_or(false),
    }
}
pub fn fetch(services: &Services, track: &Track) -> Result<Option<Lyrics>> {
    let url = "https://lrclib.net/api/get";
    let mut query = services
        .http
        .get(url)
        .header(
            "User-Agent",
            format!(
                "WeDex/{} (personal music player)",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .query(&[
            ("artist_name", track.artist.as_str()),
            ("track_name", track.title.as_str()),
            ("album_name", track.album.as_str()),
        ]);
    if (1..=3600).contains(&track.duration) {
        query = query.query(&[("duration", track.duration)]);
    }
    let exact = request_raw(query)?;
    if exact.status() != reqwest::StatusCode::NOT_FOUND {
        let value: Value = response(Ok(exact))?.json().map_err(|e| e.without_url())?;
        let lyrics = decode(&value);
        if lyrics.instrumental || !lyrics.lines.is_empty() || !lyrics.plain.is_empty() {
            return Ok(Some(lyrics));
        }
    }
    // An absent exact signature is normal; search still checks recording identity
    // and duration before selecting a result.
    let values: Vec<Value> = request(
        services
            .http
            .get("https://lrclib.net/api/search")
            .query(&[("artist_name", &track.artist), ("track_name", &track.title)]),
    )?
    .json()
    .map_err(|e| e.without_url())?;
    let selected = values
        .iter()
        .filter(|v| {
            normalize(v["trackName"].as_str().unwrap_or_default()) == normalize(&track.title)
                && normalize(v["artistName"].as_str().unwrap_or_default())
                    == normalize(&track.artist)
        })
        .filter(|v| {
            track.duration == 0
                || (v["duration"].as_f64().unwrap_or(0.) - track.duration as f64).abs() <= 4.
        })
        .min_by(|a, b| {
            let rank = |v: &Value| {
                (
                    !v["syncedLyrics"].is_string(),
                    (v["duration"].as_f64().unwrap_or(0.) - track.duration as f64).abs(),
                )
            };
            let a = rank(a);
            let b = rank(b);
            a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
        });
    Ok(selected.map(decode))
}
#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Queries the public LRCLIB API without account credentials"]
    fn live_plain_and_synchronized_lyrics() {
        let services = crate::providers::Services::new(String::new(), String::new(), String::new());
        let track = crate::model::tests::track("Blinding Lights", "The Weeknd", 200);
        let lyrics = super::fetch(&services, &track)
            .unwrap()
            .expect("Known recording has lyrics");
        assert!(!lyrics.plain.is_empty());
        assert!(!lyrics.lines.is_empty());
        println!(
            "LRCLIB: {} synchronized lines, plain text available",
            lyrics.lines.len()
        );
    }
    #[test]
    fn timestamps_offsets_repeats_and_seeks() {
        let l = super::Lyrics {
            lines: super::parse_lrc(
                "[offset:500]\n[00:01.00][00:03.25]A\n[00:02.00]B\n[ar:Example]",
            ),
            ..Default::default()
        };
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.active(0.), None);
        assert_eq!(l.active(2.6), Some(1));
        assert_eq!(l.active(4.), Some(2));
        assert_eq!(l.active(1.6), Some(0));
        assert_eq!(l.lines[2].text, "A");
    }
}
