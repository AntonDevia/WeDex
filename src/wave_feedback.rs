//! Bounded, ordered feedback, measured from audible time rather than seek position.
use crate::{
    audio::Status,
    model::Provider,
    providers::{wave::Session, Services},
};
use std::{
    sync::mpsc::{self, SyncSender},
    time::Instant,
};
struct Message {
    services: Services,
    session: Session,
    kind: &'static str,
    track: Option<String>,
    seconds: f32,
}
struct Playing {
    session: Session,
    services: Services,
    id: String,
    ticket: u64,
    seconds: f32,
    playing: bool,
    ended: bool,
    position: f32,
}
pub struct Feedback {
    tx: SyncSender<Message>,
    current: Option<Playing>,
    at: Instant,
    started: Option<String>,
}
impl Feedback {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Message>(64);
        std::thread::spawn(move || {
            while let Ok(m) = rx.recv() {
                if let Err(e) =
                    m.services
                        .wave_feedback(&m.session, m.kind, m.track.as_deref(), m.seconds)
                {
                    let _ = crate::diagnostics::append(
                        &crate::diagnostics::log_path(),
                        "wave-feedback",
                        &e.to_string(),
                        &[&m.services.yandex],
                    );
                }
            }
        });
        Self {
            tx,
            current: None,
            at: Instant::now(),
            started: None,
        }
    }
    fn send(&self, p: &Playing, kind: &'static str, track: bool) {
        let _ = self.tx.try_send(Message {
            services: p.services.clone(),
            session: p.session.clone(),
            kind,
            track: track.then(|| p.id.clone()),
            seconds: p.seconds,
        });
    }
    pub fn finish(&mut self) {
        if let Some(p) = self.current.take() {
            self.send(&p, if p.ended { "trackFinished" } else { "skip" }, true);
        }
    }
    pub fn like(&self, id: &str, liked: bool) {
        if let Some(p) = self.current.as_ref().filter(|p| p.id == id) {
            self.send(p, if liked { "like" } else { "unlike" }, true);
        }
    }
    pub fn observe(&mut self, status: &Status, session: Option<&Session>, services: &Services) {
        self.observe_at(status, session, services, Instant::now());
    }
    fn observe_at(
        &mut self,
        status: &Status,
        session: Option<&Session>,
        services: &Services,
        now: Instant,
    ) {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f32();
        self.at = now;
        if let Some(p) = &mut self.current {
            if p.ticket == status.ticket {
                let advance = status.position - p.position;
                if p.playing
                    && status.playing
                    && !status.loading
                    && elapsed < 2.
                    && advance >= 0.
                    && advance <= elapsed + 0.15
                {
                    p.seconds += advance.min(elapsed);
                }
                p.position = status.position;
                p.playing = status.playing && !status.loading;
                p.ended = status.ended;
                return;
            }
        }
        self.finish();
        let Some((s, t)) = session.zip(status.track.as_ref()).filter(|(s, t)| {
            status.playing
                && !status.loading
                && t.provider == Provider::Yandex
                && s.batches.contains_key(&t.id)
        }) else {
            return;
        };
        let p = Playing {
            session: s.clone(),
            services: services.clone(),
            id: t.id.clone(),
            ticket: status.ticket,
            seconds: 0.,
            playing: true,
            ended: false,
            position: status.position,
        };
        if self.started.as_ref() != Some(&s.id) {
            self.send(&p, "radioStarted", false);
            self.started = Some(s.id.clone());
        }
        self.send(&p, "trackStarted", true);
        self.current = Some(p);
    }
}
impl Drop for Feedback {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_is_ordered_and_counts_audio_without_seeks_pause_or_preload() {
        let (tx, rx) = mpsc::sync_channel(64);
        let now = Instant::now();
        let mut f = Feedback {
            tx,
            current: None,
            at: now,
            started: None,
        };
        let service = Services::new(String::new(), String::new(), String::new());
        let track = crate::model::tests::track("Song", "Artist", 200);
        let session = Session {
            id: "fixture-session".into(),
            batches: [(track.id.clone(), "batch".into())].into(),
            received: vec![track.id.clone()],
        };
        let mut status = Status {
            ticket: 1,
            track: Some(track),
            loading: true,
            ..Default::default()
        };
        f.observe_at(&status, Some(&session), &service, now);
        assert!(rx.try_recv().is_err());
        status.playing = true;
        status.loading = false;
        f.observe_at(&status, Some(&session), &service, now);
        assert_eq!(rx.try_recv().unwrap().kind, "radioStarted");
        assert_eq!(rx.try_recv().unwrap().kind, "trackStarted");
        status.position = 1.;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(1),
        );
        status.position = 100.;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(2),
        );
        status.playing = false;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(3),
        );
        status.playing = true;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(4),
        );
        status.position = 101.;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(5),
        );
        f.like(&status.track.as_ref().unwrap().id, true);
        assert_eq!(rx.try_recv().unwrap().kind, "like");
        status.ended = true;
        status.playing = false;
        f.observe_at(
            &status,
            Some(&session),
            &service,
            now + std::time::Duration::from_secs(6),
        );
        f.finish();
        let end = rx.try_recv().unwrap();
        assert_eq!(end.kind, "trackFinished");
        assert_eq!(end.seconds, 2.);
    }
}
