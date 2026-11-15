//! Yandex recommendation sessions. Catalog IDs stay intact when audio is supplied elsewhere.
use super::*;
use serde_json::json;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Session {
    pub id: String,
    pub batches: HashMap<String, String>,
    pub received: Vec<String>,
}
pub struct Batch {
    pub session: Session,
    pub tracks: Vec<Track>,
}

impl Services {
    fn wave_post(&self, path: &str, body: Value) -> Result<Value> {
        anyhow::ensure!(!self.yandex.is_empty(), "Яндекс не подключён");
        let value = request_json(
            self.http
                .post(format!("https://api.music.yandex.net/rotor/{path}"))
                .header("Authorization", format!("OAuth {}", self.yandex))
                .json(&body),
        )?;
        if value.get("error").is_some() {
            bail!("Волна: {}", text(&value["error"], "message"));
        }
        value
            .get("result")
            .cloned()
            .context("Волна вернула пустой ответ")
    }
    pub fn wave(
        &self,
        session: Option<Session>,
        seed: Option<&Track>,
        discovery: f32,
        excluded: &[String],
    ) -> Result<Batch> {
        if let Some(s) = session {
            if let Ok(value) = self.wave_post(
                &format!("session/{}/tracks", url_id(&s.id)?),
                json!({"queue":s.received}),
            ) {
                if value["unknownSession"] != true {
                    if let Ok(batch) = parse(value, s) {
                        return Ok(batch);
                    }
                }
            }
        }
        let mut seeds = vec!["user:onyourwave".to_owned()];
        if let Some(t) = seed {
            let catalog = if t.provider == Provider::Yandex {
                Some(t.clone())
            } else {
                self.yandex_search(&t.query())?
                    .into_iter()
                    .find(|candidate| match_score(t, candidate) >= 0.94)
            };
            if let Some(t) = catalog {
                seeds = vec![format!("track:{}", t.id)];
            }
        }
        // Diversity seeds are understood by the session API and apply to this session only.
        if discovery >= 0.6 {
            seeds.push("settingDiversity:discover".into());
        }
        let value = self.wave_post("session/new", json!({"seeds":seeds,"queue":excluded,
            "includeTracksInResponse":true,"includeWaveModel":true,"interactive":true,"allowExplicit":true}))?;
        parse(
            value,
            Session {
                received: excluded.to_vec(),
                ..Default::default()
            },
        )
    }
    pub fn wave_feedback(
        &self,
        session: &Session,
        kind: &str,
        track: Option<&str>,
        seconds: f32,
    ) -> Result<()> {
        let stamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let mut event = json!({"type":kind,"timestamp":stamp});
        if let Some(id) = track {
            event["trackId"] = json!(id);
        }
        if matches!(kind, "skip" | "trackFinished") {
            event["totalPlayedSeconds"] = json!(seconds.max(0.));
        }
        let mut body = json!({"event":event,"from":"radio-mobile-user-onyourwave"});
        if let Some(batch) = track.and_then(|id| session.batches.get(id)) {
            body["batchId"] = json!(batch);
        }
        self.wave_post(&format!("session/{}/feedback", url_id(&session.id)?), body)?;
        Ok(())
    }
}
fn url_id(id: &str) -> Result<&str> {
    anyhow::ensure!(
        !id.is_empty()
            && id.len() < 512
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c)),
        "Некорректная сессия Волны"
    );
    Ok(id)
}
fn parse(value: Value, mut session: Session) -> Result<Batch> {
    if let Some(id) = value["radioSessionId"].as_str() {
        session.id = id.into();
    }
    url_id(&session.id)?;
    let batch = value["batchId"].as_str().context("Нет партии Волны")?;
    let tracks = value["sequence"]
        .as_array()
        .context("Нет треков Волны")?
        .iter()
        .filter_map(|item| super::yandex::parse_track(&item["track"]))
        .filter(|t| !session.received.contains(&t.id))
        .collect::<Vec<_>>();
    anyhow::ensure!(!tracks.is_empty(), "Волна пока не вернула новых треков");
    for track in &tracks {
        session.batches.insert(track.id.clone(), batch.into());
        session.received.push(track.id.clone());
    }
    // Bounded session history; feedback for the live queue remains available.
    if session.received.len() > 400 {
        let expired = session
            .received
            .drain(..session.received.len() - 400)
            .collect::<Vec<_>>();
        for id in expired {
            session.batches.remove(&id);
        }
    }
    Ok(Batch { session, tracks })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_queue_rejects_duplicates_and_tracks_feedback_batches() {
        let value = json!({"radioSessionId":"session-1","batchId":"batch-1","sequence":[{"track":{"id":123,"title":"Song","durationMs":200000,"artists":[{"name":"Artist"}],"albums":[]}}]});
        let first = parse(value.clone(), Session::default()).unwrap();
        assert_eq!(first.tracks[0].provider, Provider::Yandex);
        assert_eq!(first.session.batches["123"], "batch-1");
        assert!(parse(value, first.session).is_err());
        assert!(url_id("../not-a-session").is_err());
    }
    #[test]
    #[ignore = "Starts a real personal Wave session using the configured account, without playback feedback"]
    fn live_personal_wave() {
        let services = Services::new(crate::store::secret("yandex"), String::new(), String::new());
        let batch = services.wave(None, None, 0., &[]).unwrap();
        assert!(!batch.tracks.is_empty());
        let second = services
            .wave(Some(batch.session.clone()), None, 0., &[])
            .unwrap();
        assert!(second
            .tracks
            .iter()
            .all(|t| !batch.session.received.contains(&t.id)));
        println!(
            "Personal Wave: {} initial, {} additional tracks",
            batch.tracks.len(),
            second.tracks.len()
        );
    }
}
