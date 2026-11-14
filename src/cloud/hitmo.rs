//! Check reachability from this device before changing its catalogue setting.
use crate::providers::Services;
use std::{
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use wedex_cloud::mirror::{valid_origin, HitmoMirror};

#[derive(Default)]
pub struct Discovery {
    attempt: Option<(String, String, Instant)>,
    pending: Option<(String, Receiver<Option<String>>)>,
}
impl Discovery {
    pub fn offer(&mut self, mirror: HitmoMirror, current: &str) {
        if !mirror.valid(wedex_cloud::now_ms())
            || !valid_origin(current)
            || mirror.origin == current
            || self.pending.is_some()
            || self.attempt.as_ref().is_some_and(|(from, to, at)| {
                from == current
                    && to == &mirror.origin
                    && at.elapsed() < Duration::from_secs(15 * 60)
            })
        {
            return;
        }
        self.attempt = Some((current.into(), mirror.origin.clone(), Instant::now()));
        let (send, receive) = mpsc::channel();
        let from = current.to_owned();
        let origin = mirror.origin;
        std::thread::spawn(move || {
            let checked = (|| -> anyhow::Result<Option<String>> {
                let mut services = Services::new(String::new(), String::new(), String::new());
                services.http = reqwest::blocking::Client::builder()
                    .connect_timeout(Duration::from_secs(4))
                    .timeout(Duration::from_secs(8))
                    .redirect(reqwest::redirect::Policy::none())
                    .user_agent("Mozilla/5.0 (compatible; WeDexMirror/1.0)")
                    .build()?;
                services.hitmo_base = from;
                // A healthy existing mirror keeps working, including manual choices.
                if services
                    .hitmotop_search("music")
                    .is_ok_and(|t| !t.is_empty())
                {
                    return Ok(None);
                }
                services.hitmo_base = origin.clone();
                anyhow::ensure!(
                    !services.hitmotop_search("music")?.is_empty(),
                    "Mirror returned no tracks"
                );
                Ok(Some(origin))
            })();
            // Discovery failure leaves the last setting intact, without interrupting playback.
            if let Err(e) = &checked {
                let _ = crate::diagnostics::append(
                    &crate::diagnostics::log_path(),
                    "hitmo-mirror",
                    &e.to_string(),
                    &[],
                );
            }
            let _ = send.send(checked.unwrap_or(None));
        });
        self.pending = Some((current.into(), receive));
    }
    pub fn take(&mut self, current: &str) -> Option<String> {
        let (from, receive) = self.pending.as_ref()?;
        match receive.try_recv() {
            Ok(value) => {
                let unchanged = from == current;
                self.pending = None;
                value.filter(|_| unchanged)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                None
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_probe_cannot_overwrite_a_user_edit() {
        let (tx, rx) = mpsc::channel();
        let mut d = Discovery {
            pending: Some(("https://ru.hitmo-top.cc/".into(), rx)),
            ..Default::default()
        };
        tx.send(Some("https://ru.hitmox.net/".into())).unwrap();
        assert!(d.take("https://rus.hitmos.fm/").is_none());
        assert!(d.pending.is_none());
    }
}
