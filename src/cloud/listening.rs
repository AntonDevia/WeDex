use crate::{audio::Status, store::Store};
use std::time::Instant;
use wedex_cloud::Session;

pub struct Listening {
    ticket: u64,
    session: Option<Session>,
    previous: Option<(Instant, f32, bool)>,
    checkpoint: Instant,
    dirty: bool,
}
impl Default for Listening {
    fn default() -> Self {
        Self {
            ticket: 0,
            session: None,
            previous: None,
            checkpoint: Instant::now(),
            dirty: false,
        }
    }
}
impl Listening {
    pub fn tick(
        &mut self,
        status: &Status,
        store: Option<&Store>,
        now: Instant,
    ) -> anyhow::Result<()> {
        let running = status.playing && !status.loading && !status.ended && status.error.is_none();
        if self.ticket != status.ticket {
            self.flush(store)?;
            self.ticket = status.ticket;
            self.session = status.track.as_ref().map(|track| Session {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 0,
                track: super::track(track),
                started_ms: wedex_cloud::now_ms(),
                listened_ms: 0,
            });
            self.previous = None;
        }
        if let Some((previous_at, position, was_running)) = self.previous {
            let elapsed = now.saturating_duration_since(previous_at).as_secs_f32();
            if let Some(session) = &mut self.session {
                let advanced = status.position - position;
                // Count audible progress, not a seek jump, pause, buffering or a sleeping UI.
                if was_running
                    && running
                    && elapsed < 2.
                    && advanced >= 0.
                    && advanced <= elapsed + 0.15
                {
                    let milliseconds = (advanced.min(elapsed) * 1000.).round() as u64;
                    session.listened_ms = session.listened_ms.saturating_add(milliseconds);
                    self.dirty |= milliseconds > 0;
                }
            }
            if was_running && !running {
                self.flush(store)?;
            }
        }
        self.previous = Some((now, status.position, running));
        if now.saturating_duration_since(self.checkpoint).as_secs() >= 5 {
            self.flush(store)?;
            self.checkpoint = now;
        }
        Ok(())
    }
    pub fn discontinuity(&mut self) {
        self.previous = None;
    }
    pub fn flush(&mut self, store: Option<&Store>) -> anyhow::Result<()> {
        if self.dirty {
            if let (Some(store), Some(session)) = (store, &mut self.session) {
                session.revision += 1;
                store.save_session(session)?;
                self.dirty = false;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_buffering_and_seek_do_not_inflate_audible_time() {
        let mut listening = Listening::default();
        let mut status = Status {
            ticket: 1,
            playing: true,
            track: Some(crate::model::tests::track("song", "artist", 200)),
            ..Default::default()
        };
        let now = Instant::now();
        listening.tick(&status, None, now).unwrap();
        status.position = 1.;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(1))
            .unwrap();
        status.position = 100.;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(2))
            .unwrap();
        status.playing = false;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(3))
            .unwrap();
        status.playing = true;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(4))
            .unwrap();
        status.position = 101.;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(5))
            .unwrap();
        status.loading = true;
        status.position = 102.;
        listening
            .tick(&status, None, now + std::time::Duration::from_secs(6))
            .unwrap();
        assert_eq!(listening.session.unwrap().listened_ms, 2000);
    }
}
