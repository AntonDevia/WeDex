use crate::{
    fingerprinting::algorithm::SignatureGenerator,
    model::{Edition, Provider, Track},
    providers::{json, text, Services},
};
use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CAPTURE_SECONDS: u64 = 12;

#[derive(Clone, Copy, Default)]
pub(crate) struct Visual {
    pub progress: f32,
    pub level: f32,
    pub searching: bool,
}

#[derive(Default)]
pub(crate) struct Capture {
    cancelled: AtomicBool,
    progress: AtomicU32,
    level: AtomicU32,
    searching: AtomicBool,
}

impl Capture {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn visual(&self) -> Visual {
        Visual {
            progress: f32::from_bits(self.progress.load(Ordering::Relaxed)),
            level: f32::from_bits(self.level.load(Ordering::Relaxed)),
            searching: self.searching.load(Ordering::Relaxed),
        }
    }

    fn append<T: Copy>(
        &self,
        samples: &Mutex<Vec<f32>>,
        input: &[T],
        limit: usize,
        to_float: impl Fn(T) -> f32,
    ) {
        let mut samples = samples.lock().unwrap();
        let start = samples.len();
        let count = (limit - start).min(input.len());
        if count == 0 {
            return;
        }
        samples.extend(input[..count].iter().copied().map(to_float));
        let energy = samples[start..].iter().map(|s| s * s).sum::<f32>() / count as f32;
        let db = 10. * energy.max(1.0e-10).log10();
        let target = ((db + 55.) / 43.).clamp(0., 1.);
        let previous = f32::from_bits(self.level.load(Ordering::Relaxed));
        let smoothing = if target > previous { 0.58 } else { 0.18 };
        let level = previous + (target - previous) * smoothing;
        self.level.store(level.to_bits(), Ordering::Relaxed);
        let progress = samples.len() as f32 / limit as f32;
        self.progress.store(progress.to_bits(), Ordering::Relaxed);
    }
}

/// Records only after an explicit button click. Audio stays in RAM; only a fingerprint is uploaded.
pub(crate) fn recognize(services: &Services, capture: Arc<Capture>) -> Result<Track> {
    let device = cpal::default_host()
        .default_input_device()
        .context("Микрофон не найден. Проверьте разрешение на запись звука")?;
    let config = device.default_input_config()?;
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let data = samples.clone();
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let max_samples = rate as usize * channels * CAPTURE_SECONDS as usize;
    let capture_error = Arc::new(Mutex::new(None::<String>));
    let error = capture_error.clone();
    let err = move |e: cpal::StreamError| {
        *error.lock().unwrap() = Some(e.to_string());
    };
    let meter = capture.clone();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |input: &[f32], _| {
                meter.append(&data, input, max_samples, |s| s);
            },
            err,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |input: &[i16], _| {
                meter.append(&data, input, max_samples, |s| s as f32 / 32768.0);
            },
            err,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |input: &[u16], _| {
                meter.append(&data, input, max_samples, |s| {
                    (s as f32 - 32768.0) / 32768.0
                });
            },
            err,
            None,
        )?,
        _ => bail!("Формат микрофона не поддерживается"),
    };
    stream.play()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(CAPTURE_SECONDS) {
        if capture.cancelled.load(Ordering::Relaxed) {
            drop(stream);
            bail!("Распознавание отменено");
        }
        if samples.lock().unwrap().len() >= max_samples {
            break;
        }
        if capture_error.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    drop(stream);
    if let Some(error) = capture_error.lock().unwrap().take() {
        bail!("Микрофон: {error}");
    }
    let input = samples.lock().unwrap();
    if input.len() < rate as usize * channels * 3 {
        bail!("Недостаточно звука с микрофона");
    }
    capture.searching.store(true, Ordering::Relaxed);
    let mono: Vec<f32> = input
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    // Windowed-sinc low pass resampling prevents high-frequency aliasing at 16 kHz.
    let pcm = resample(&mono, rate);
    let samplems = pcm.len() as u64 * 1000 / 16000;
    let signature = SignatureGenerator::make_signature_from_buffer(pcm);
    let uri = signature
        .encode_to_uri()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if capture.cancelled.load(Ordering::Relaxed) {
        bail!("Распознавание отменено");
    }
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let url = format!("https://amp.shazam.com/discovery/v5/en-US/GB/iphone/-/tag/{}/{}?sync=true&webv3=true&sampling=true&shazamapiversion=v3",uuid::Uuid::new_v4(),uuid::Uuid::new_v4());
    let v = json(services.http.post(url).header("X-Shazam-Platform","IPHONE").header("X-Shazam-AppVersion","14.1.0").json(&serde_json::json!({"timezone":"Europe/Samara","signature":{"uri":uri,"samplems":samplems},"timestamp":timestamp,"context":{},"geolocation":{}})).send())?;
    let t = v
        .get("track")
        .context("Трек не распознан. Попробуйте более громкий фрагмент")?;
    let title = text(t, "title");
    let artist = text(t, "subtitle");
    if title.is_empty() {
        bail!("Распознавание не вернуло название трека");
    }
    Ok(Track {
        album: String::new(),
        id: text(t, "key"),
        provider: Provider::LastFm,
        title,
        artist,
        duration: 0,
        cover: t
            .pointer("/images/coverarthq")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        page_url: text(t, "url"),
        edition: Edition::Unknown,
        reason: "Найдено по звуку".into(),
        score: 0.0,
    })
}

fn resample(input: &[f32], rate: u32) -> Vec<i16> {
    let ratio = rate as f64 / 16000.0;
    let cutoff = (16000.0 / rate as f64).min(1.0) * 0.94;
    let count = (input.len() as f64 / ratio) as usize;
    (0..count)
        .map(|i| {
            let pos = i as f64 * ratio;
            let center = pos.floor() as i64;
            let mut sum = 0.0;
            let mut weight = 0.0;
            for offset in -32..=32 {
                let at = center + offset;
                if at < 0 || at >= input.len() as i64 {
                    continue;
                }
                let x = pos - at as f64;
                let sinc = if x.abs() < 1e-8 {
                    cutoff
                } else {
                    (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                };
                let w = sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / 33.0).cos());
                sum += input[at as usize] as f64 * w;
                weight += w;
            }
            (sum / weight * 32767.0).clamp(-32768.0, 32767.0) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_reports_progress_and_smooths_the_audio_level() {
        let capture = Capture::default();
        let samples = Mutex::new(Vec::new());
        capture.append(&samples, &[0.25; 25], 100, |level| level);
        let loud = capture.visual();
        assert_eq!(loud.progress, 0.25);
        assert!(loud.level > 0.5 && loud.level < 1.);
        capture.append(&samples, &[0.; 100], 100, |level| level);
        let quiet = capture.visual();
        assert_eq!(quiet.progress, 1.);
        assert!(quiet.level > 0. && quiet.level < loud.level);
        assert_eq!(samples.into_inner().unwrap().len(), 100);
    }
    #[test]
    fn resampling_preserves_length_and_suppresses_aliases() {
        let sine = |hz: f64| {
            (0..48000)
                .map(|i| (std::f64::consts::TAU * hz * i as f64 / 48000.0).sin() as f32 * 0.5)
                .collect::<Vec<_>>()
        };
        let rms = |a: Vec<i16>| {
            (a[100..a.len() - 100]
                .iter()
                .map(|s| (*s as f64).powi(2))
                .sum::<f64>()
                / (a.len() - 200) as f64)
                .sqrt()
        };
        let pass = resample(&sine(1000.0), 48000);
        assert_eq!(pass.len(), 16000);
        assert!(rms(resample(&sine(12000.0), 48000)) < rms(pass) * 0.03);
    }
}
