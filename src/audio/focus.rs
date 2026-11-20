//! Audio-focus attenuation is separate from user volume and track crossfades.
use super::ramp;
#[cfg(any(target_os = "android", test))]
use std::time::Duration;
use std::time::Instant;
#[cfg(any(target_os = "android", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    Gain,
    Duck,
    TransientLoss,
    Loss,
}
pub(super) struct Focus {
    pub gain: ramp::Control,
    pub blocked: bool,
    pause_at: Option<Instant>,
}
impl Default for Focus {
    fn default() -> Self {
        Self {
            gain: ramp::control(1., 0.),
            blocked: false,
            pause_at: None,
        }
    }
}
impl Focus {
    #[cfg(any(target_os = "android", test))]
    pub fn change(&mut self, change: Change, now: Instant) {
        match change {
            Change::Gain => {
                self.blocked = false;
                self.pause_at = None;
                ramp::set(&self.gain, 1., 0.2);
            }
            Change::Duck if !self.blocked => ramp::set(&self.gain, 0.2, 0.2),
            Change::Duck => {}
            Change::TransientLoss | Change::Loss => {
                self.blocked = true;
                // Repeated loss events must not postpone the pause indefinitely.
                self.pause_at
                    .get_or_insert(now + Duration::from_millis(200));
                ramp::set(&self.gain, 0., 0.2);
            }
        }
    }
    pub fn pause_due(&mut self, now: Instant) -> bool {
        if self.pause_at.is_some_and(|at| now >= at) {
            self.pause_at = None;
            true
        } else {
            false
        }
    }
    pub fn initial_gain(&self) -> f32 {
        self.gain.read().unwrap().target
    }
}
#[cfg(target_os = "android")]
static PLAYER: std::sync::Mutex<Option<std::sync::mpsc::Sender<super::Command>>> =
    std::sync::Mutex::new(None);
#[cfg(target_os = "android")]
pub(super) fn register(sender: std::sync::mpsc::Sender<super::Command>) {
    *PLAYER.lock().unwrap() = Some(sender);
}
#[cfg(target_os = "android")]
pub(crate) fn notify(change: i32) {
    let change = match change {
        1 => Change::Gain,
        -3 => Change::Duck,
        -2 => Change::TransientLoss,
        -1 => Change::Loss,
        _ => return,
    };
    if let Some(sender) = PLAYER.lock().unwrap().as_ref() {
        let _ = sender.send(super::Command::Focus(change));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_attenuate_without_pausing_and_restore_continuously() {
        let mut focus = Focus::default();
        let now = Instant::now();
        let source = rodio::buffer::SamplesBuffer::new(1, 1000, vec![1_f32; 1000]);
        let mut audio = ramp::Smooth::new(source, focus.gain.clone(), 1.);
        focus.change(Change::Duck, now);
        let quiet = audio.by_ref().take(320).collect::<Vec<_>>();
        assert!(!focus.blocked && !focus.pause_due(now + Duration::from_secs(5)));
        assert!((quiet[0] - 1.).abs() < 0.001 && (quiet[200] - 0.2).abs() < 0.001);
        focus.change(Change::Gain, now + Duration::from_secs(1));
        let loud = audio.take(320).collect::<Vec<_>>();
        assert!((loud[0] - 0.2).abs() < 0.001 && (loud[220] - 1.).abs() < 0.001);
        assert!(quiet.iter().chain(&loud).all(|v| v.is_finite()));
        assert!(quiet
            .windows(2)
            .chain(loud.windows(2))
            .all(|v| (v[0] - v[1]).abs() < 0.005));
    }
    #[test]
    fn transient_pause_waits_for_fade_and_gain_cancels_a_stale_pause() {
        let mut focus = Focus::default();
        let now = Instant::now();
        focus.change(Change::TransientLoss, now);
        assert!(focus.blocked && !focus.pause_due(now + Duration::from_millis(199)));
        focus.change(Change::Gain, now + Duration::from_millis(100));
        assert!(!focus.blocked && !focus.pause_due(now + Duration::from_secs(1)));
        focus.change(Change::Loss, now);
        focus.change(Change::Loss, now + Duration::from_millis(100));
        assert!(focus.pause_due(now + Duration::from_millis(200)));
        focus.change(Change::Duck, now);
        assert!(focus.blocked && focus.initial_gain() == 0.);
    }
}
