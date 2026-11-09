use super::*;
#[derive(Clone, Copy)]
pub(super) struct Ramp {
    pub target: f32,
    pub seconds: f32,
    pub revision: u64,
}
pub(super) type Control = Arc<RwLock<Ramp>>;
pub(super) fn control(target: f32, seconds: f32) -> Control {
    Arc::new(RwLock::new(Ramp {
        target,
        seconds,
        revision: 1,
    }))
}
pub(super) fn set(control: &Control, target: f32, seconds: f32) {
    let mut ramp = control.write().unwrap();
    ramp.target = target;
    ramp.seconds = seconds;
    ramp.revision += 1;
}
pub(super) struct Smooth<S> {
    source: S,
    control: Control,
    gain: f32,
    step: f32,
    left: u64,
    revision: u64,
    samples: u64,
}
impl<S: Source<Item = f32>> Smooth<S> {
    pub fn new(source: S, control: Control, gain: f32) -> Self {
        Self {
            source,
            control,
            gain,
            step: 0.,
            left: 0,
            revision: 0,
            samples: 0,
        }
    }
}
impl<S: Source<Item = f32>> Iterator for Smooth<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.source.next()?;
        if self.samples.is_multiple_of(64) {
            let ramp = *self.control.read().unwrap();
            if ramp.revision != self.revision {
                self.left = (ramp.seconds
                    * self.source.sample_rate() as f32
                    * self.source.channels() as f32)
                    .round() as u64;
                self.step = if self.left > 0 {
                    (ramp.target - self.gain) / self.left as f32
                } else {
                    0.
                };
                if self.left == 0 {
                    self.gain = ramp.target;
                }
                self.revision = ramp.revision;
            }
        }
        let output = sample * self.gain;
        if self.left > 0 {
            self.gain += self.step;
            self.left -= 1;
        }
        self.samples += 1;
        Some(output)
    }
}
impl<S: Source<Item = f32>> Source for Smooth<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.source.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.source.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.source.try_seek(pos)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crossfade_gains_are_continuous_and_complementary() {
        let a = rodio::buffer::SamplesBuffer::new(1, 1000, vec![1_f32; 1001]);
        let b = rodio::buffer::SamplesBuffer::new(1, 1000, vec![1_f32; 1001]);
        let out = Smooth::new(a, control(0., 1.), 1.).collect::<Vec<_>>();
        let incoming = Smooth::new(b, control(1., 1.), 0.).collect::<Vec<_>>();
        assert!(incoming[0].abs() < 1e-6);
        assert!((incoming[500] - 0.5).abs() < 1e-4);
        assert!((incoming[1000] - 1.).abs() < 1e-4);
        assert!(incoming.windows(2).all(|s| (s[1] - s[0]).abs() <= 0.0011));
        assert!(incoming
            .iter()
            .zip(out)
            .all(|(a, b)| (a + b - 1.).abs() < 1e-4));
    }
}
