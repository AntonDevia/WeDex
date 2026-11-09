mod download;
mod engine;
mod preload;
mod ramp;
use crate::{
    model::{Provider, Track},
    providers::{response, Services},
};
use anyhow::{bail, Context, Result};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
        Arc, Condvar, Mutex, RwLock,
    },
    time::Duration,
};

pub const BANDS: [f32; 10] = crate::equalizer::FREQUENCIES;

#[derive(Clone, Default)]
pub struct Status {
    pub ticket: u64,
    pub loading: bool,
    pub playing: bool,
    pub ended: bool,
    pub position: f32,
    pub duration: f32,
    pub track: Option<Track>,
    pub error: Option<String>,
}

type Decoded = Decoder<Progressive>;
enum Command {
    Play(u64, Track, Services, Option<preload::Warm>, f32),
    Ready(
        u64,
        Track,
        Box<Decoded>,
        SharedBuffer,
        Option<Arc<AtomicU64>>,
        f32,
    ),
    Failed(u64, String),
    Pause,
    SetPlaying(bool),
    Seek(f32),
    Volume(f32),
    Stop,
    Quit,
}

pub struct Player {
    #[cfg(test)]
    silent: bool,
    warm: Mutex<Option<preload::Warm>>,
    tx: Sender<Command>,
    state: Arc<Mutex<Status>>,
    generation: Arc<AtomicU64>,
    pub eq: Arc<RwLock<[f32; 10]>>,
    pub preamp: Arc<RwLock<f32>>,
}
impl Player {
    pub fn new(volume: f32, eq: [f32; 10]) -> Self {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Status::default()));
        let generation = Arc::new(AtomicU64::new(0));
        let equalizer = Arc::new(RwLock::new(eq));
        let preamp = Arc::new(RwLock::new(0.));
        let shared = state.clone();
        let gen = generation.clone();
        let shared_eq = equalizer.clone();
        let shared_preamp = preamp.clone();
        let results = tx.clone();
        std::thread::spawn(move || {
            engine::run(rx, shared, gen, shared_eq, shared_preamp, results, volume)
        });
        Self {
            #[cfg(test)]
            silent: false,
            warm: Mutex::new(None),
            tx,
            state,
            generation,
            eq: equalizer,
            preamp,
        }
    }
    pub fn transition(&self, track: Track, services: Services, seconds: f32) -> u64 {
        let ticket = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        #[cfg(test)]
        if self.silent {
            *self.state.lock().unwrap() = Status {
                ticket,
                playing: true,
                duration: track.duration as f32,
                track: Some(track),
                ..Default::default()
            };
            return ticket;
        }
        let warm = self.warm.lock().unwrap().take();
        let warm = warm.and_then(|w| {
            if w.key == track.key() {
                Some(w)
            } else {
                w.cancel.store(2, Ordering::Relaxed);
                None
            }
        });
        let _ = self
            .tx
            .send(Command::Play(ticket, track, services, warm, seconds));
        ticket
    }
    pub fn preload(&self, track: Option<Track>, services: Services) {
        #[cfg(test)]
        if self.silent {
            return;
        }
        let mut warm = self.warm.lock().unwrap();
        if warm.as_ref().map(|w| w.key.as_str()) == track.as_ref().map(|t| t.key()).as_deref() {
            return;
        }
        if let Some(old) = warm.take() {
            old.cancel.store(2, Ordering::Relaxed);
        }
        *warm = track.map(|t| preload::Warm::start(t, services));
    }
    pub fn preloaded(&self, key: &str) -> bool {
        self.warm
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|w| w.key == key && w.complete())
    }
    pub fn pause(&self) {
        let _ = self.tx.send(Command::Pause);
    }
    pub fn seek(&self, pos: f32) {
        let _ = self.tx.send(Command::Seek(pos));
    }
    pub fn volume(&self, volume: f32) {
        let _ = self.tx.send(Command::Volume(volume));
    }
    pub fn set_playing(&self, playing: bool) {
        let _ = self.tx.send(Command::SetPlaying(playing));
    }
    pub fn status(&self) -> Status {
        self.state.lock().unwrap().clone()
    }
    pub fn stop(&self) {
        #[cfg(test)]
        if self.silent {
            *self.state.lock().unwrap() = Status::default();
        }
        if let Some(w) = self.warm.lock().unwrap().take() {
            w.cancel.store(2, Ordering::Relaxed);
        }
        self.generation.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Command::Stop);
    }
}
#[cfg(test)]
impl Player {
    pub fn for_tests(eq: [f32; 10]) -> Self {
        let (tx, _) = mpsc::channel();
        Self {
            silent: true,
            warm: Mutex::new(None),
            tx,
            state: Arc::new(Mutex::new(Status::default())),
            generation: Arc::new(AtomicU64::new(0)),
            eq: Arc::new(RwLock::new(eq)),
            preamp: Arc::new(RwLock::new(0.)),
        }
    }
    pub fn finish_for_test(&self) {
        let mut state = self.state.lock().unwrap();
        state.playing = false;
        state.ended = true;
        state.position = state.duration;
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        if let Some(w) = self.warm.lock().unwrap().take() {
            w.cancel.store(2, Ordering::Relaxed);
        }
        self.generation.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Command::Quit);
    }
}

const MAX_BUFFER: usize = 128 * 1024 * 1024;
struct Buffer {
    bytes: Vec<u8>,
    done: bool,
    error: Option<String>,
}
type SharedBuffer = Arc<(Mutex<Buffer>, Condvar)>;
struct Progressive {
    shared: SharedBuffer,
    pos: u64,
    length: Option<u64>,
    generation: Arc<AtomicU64>,
    ticket: u64,
}
impl Progressive {
    fn open(
        http: &reqwest::blocking::Client,
        url: &str,
        token: String,
        generation: Arc<AtomicU64>,
        ticket: u64,
    ) -> Result<Self> {
        let res = response(audio_request(http, url, &token).send())?;
        let is_hls = res
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|s| s.contains("mpegurl"))
            || res.url().path().ends_with(".m3u8");
        let mut segments = vec![];
        let mut direct = None;
        let length = if is_hls {
            let base = res.url().clone();
            let playlist = res.text().map_err(|e| e.without_url())?;
            segments = hls_segments(http, base, &playlist, &token, 0)?;
            None
        } else {
            let length = res.content_length();
            direct = Some(res);
            length
        };
        if length.is_some_and(|n| n > MAX_BUFFER as u64) {
            bail!("Запись превышает лимит буфера 128 МБ");
        }
        let shared = Arc::new((
            Mutex::new(Buffer {
                bytes: Vec::new(),
                done: false,
                error: None,
            }),
            Condvar::new(),
        ));
        let url = url.to_owned();
        let worker = shared.clone();
        let gen = generation.clone();
        let http = http.clone();
        std::thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                if let Some(direct) = direct {
                    download::stream(&http, &url, &token, Some(direct), &worker, &gen, ticket)?;
                } else {
                    for segment in segments {
                        download::stream(&http, &segment, &token, None, &worker, &gen, ticket)?;
                    }
                }
                Ok(())
            })();
            let mut buffer = worker.0.lock().unwrap();
            buffer.done = true;
            if let Err(e) = result {
                buffer.error = Some(e.to_string());
            }
            if let Some(length) = length {
                if buffer.bytes.len() as u64 != length && buffer.error.is_none() {
                    buffer.error = Some("Аудиопоток оборвался до конца записи".into());
                }
            }
            worker.1.notify_all();
        });
        Ok(Self {
            shared,
            pos: 0,
            length,
            generation,
            ticket,
        })
    }
}

fn audio_request(
    http: &reqwest::blocking::Client,
    url: &str,
    token: &str,
) -> reqwest::blocking::RequestBuilder {
    let request = http.get(url).timeout(Duration::from_secs(180));
    if !token.is_empty()
        && url::Url::parse(url)
            .ok()
            .is_some_and(|u| u.host_str() == Some("api.soundcloud.com"))
    {
        request.header("Authorization", format!("OAuth {token}"))
    } else {
        request
    }
}

fn hls_segments(
    http: &reqwest::blocking::Client,
    base: url::Url,
    body: &str,
    token: &str,
    depth: usize,
) -> Result<Vec<String>> {
    if depth > 3 {
        bail!("Слишком много вложенных HLS-плейлистов");
    }
    if !body.trim_start().starts_with("#EXTM3U") {
        bail!("Некорректный HLS-плейлист");
    }
    if body
        .lines()
        .any(|l| l.starts_with("#EXT-X-KEY:") && !l.contains("METHOD=NONE"))
    {
        bail!("Зашифрованный HLS пока не поддерживается");
    }
    if body.contains("#EXT-X-BYTERANGE") || body.contains("#EXT-X-MAP") {
        bail!("Этот формат HLS пока не поддерживается");
    }
    let urls: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| base.join(l).map(|u| u.to_string()))
        .collect::<Result<_, _>>()?;
    if body.contains("#EXT-X-STREAM-INF:") {
        let url = urls.first().context("Пустой master-плейлист HLS")?;
        let res = response(audio_request(http, url, token).send())?;
        let base = res.url().clone();
        let body = res.text().map_err(|e| e.without_url())?;
        return hls_segments(http, base, &body, token, depth + 1);
    }
    if !body.contains("#EXT-X-ENDLIST") {
        bail!("Живые HLS-трансляции пока не поддерживаются");
    }
    if urls.is_empty() || urls.len() > 5000 {
        bail!("Некорректное число HLS-сегментов");
    }
    Ok(urls)
}
impl Read for Progressive {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut data = self.shared.0.lock().unwrap();
        loop {
            if self.generation.load(Ordering::Relaxed) != self.ticket {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Загрузка отменена",
                ));
            }
            if self.pos < data.bytes.len() as u64 {
                let n = out.len().min(data.bytes.len() - self.pos as usize);
                out[..n].copy_from_slice(&data.bytes[self.pos as usize..self.pos as usize + n]);
                self.pos += n as u64;
                return Ok(n);
            }
            if data.done {
                return match &data.error {
                    Some(e) => Err(io::Error::other(e.clone())),
                    None => Ok(0),
                };
            }
            let wait = self
                .shared
                .1
                .wait_timeout(data, Duration::from_millis(100))
                .unwrap();
            data = wait.0;
        }
    }
}
impl Seek for Progressive {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
            SeekFrom::End(n) => {
                let mut data = self.shared.0.lock().unwrap();
                while self.length.is_none() && !data.done {
                    if self.generation.load(Ordering::Relaxed) != self.ticket {
                        return Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "Загрузка отменена",
                        ));
                    }
                    data = self
                        .shared
                        .1
                        .wait_timeout(data, Duration::from_millis(100))
                        .unwrap()
                        .0;
                }
                self.length.unwrap_or(data.bytes.len() as u64) as i128 + n as i128
            }
        };
        if pos < 0 || pos > MAX_BUFFER as i128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Позиция за пределами аудиобуфера",
            ));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

#[derive(Clone, Copy)]
struct Filter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}
impl Filter {
    fn peak(hz: f32, db: f32, rate: f32) -> Self {
        let a = 10_f32.powf(db / 40.0);
        let w = std::f32::consts::TAU * hz.min(rate * 0.45) / rate;
        let alpha = w.sin() / (2.0 * 1.4);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * w.cos() / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * w.cos() / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
    fn run(self, input: f32, state: &mut [f32; 2]) -> f32 {
        let y = self.b0 * input + state[0];
        state[0] = self.b1 * input - self.a1 * y + state[1];
        state[1] = self.b2 * input - self.a2 * y;
        y
    }
}
struct Equalizer<S> {
    source: S,
    gains: Arc<RwLock<[f32; 10]>>,
    last: [f32; 10],
    filters: [Filter; 10],
    states: Vec<[[f32; 2]; 10]>,
    channel: usize,
    frames: usize,
    preamp: f32,
    level: Arc<RwLock<f32>>,
    last_level: f32,
}
impl<S: Source<Item = f32>> Equalizer<S> {
    #[cfg(test)]
    fn new(source: S, gains: Arc<RwLock<[f32; 10]>>) -> Self {
        Self::with_preamp(source, gains, Arc::new(RwLock::new(0.)))
    }
    fn with_preamp(source: S, gains: Arc<RwLock<[f32; 10]>>, level: Arc<RwLock<f32>>) -> Self {
        let channels = source.channels() as usize;
        Self {
            filters: BANDS.map(|hz| Filter::peak(hz, 0.0, source.sample_rate() as f32)),
            source,
            gains,
            last: [f32::NAN; 10],
            states: vec![[[0.0; 2]; 10]; channels],
            channel: 0,
            frames: 0,
            preamp: 1.0,
            level,
            last_level: f32::NAN,
        }
    }
}
impl<S: Source<Item = f32>> Iterator for Equalizer<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let mut sample = self.source.next()?;
        if self.frames.is_multiple_of(1024 * self.states.len()) {
            let gains = *self.gains.read().unwrap();
            let level = *self.level.read().unwrap();
            if gains != self.last || level != self.last_level {
                self.filters = std::array::from_fn(|i| {
                    Filter::peak(BANDS[i], gains[i], self.source.sample_rate() as f32)
                });
                // Headroom follows positive gain, keeping EQ boosts away from digital clipping.
                self.preamp =
                    10_f32.powf((level - gains.iter().copied().fold(0.0_f32, f32::max)) / 20.0);
                self.last_level = level;
                self.last = gains;
            }
        }
        sample *= self.preamp;
        for i in 0..10 {
            sample = self.filters[i].run(sample, &mut self.states[self.channel][i]);
        }
        self.channel = (self.channel + 1) % self.states.len();
        self.frames += 1;
        Some(sample.clamp(-1.0, 1.0))
    }
}
impl<S: Source<Item = f32>> Source for Equalizer<S> {
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
        self.source.try_seek(pos)?;
        self.states.fill([[0.0; 2]; 10]);
        self.channel = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preamp_changes_signal_level_independently_of_the_ten_bands() {
        for db in [-12., 0., 12.] {
            let source = rodio::buffer::SamplesBuffer::new(1, 48000, vec![0.1_f32; 2048]);
            let gain = Arc::new(RwLock::new(db));
            let mut eq = Equalizer::with_preamp(source, Arc::new(RwLock::new([0.; 10])), gain);
            let output = eq.next().unwrap();
            assert!((output - 0.1 * 10_f32.powf(db / 20.)).abs() < 1e-5);
        }
    }
    #[test]
    fn progressively_downloaded_wav_decodes_and_seeks() {
        use std::{io::Write, net::TcpListener};
        let samples = 16000_u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&16000_u32.to_le_bytes());
        wav.extend_from_slice(&32000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        for _ in 0..samples {
            wav.extend_from_slice(&1000_i16.to_le_bytes());
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).unwrap();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",wav.len()).unwrap();
            for part in wav.chunks(512) {
                socket.write_all(part).unwrap();
            }
        });
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let input = Progressive::open(
            &http,
            &format!("http://{address}/audio.wav"),
            String::new(),
            Arc::new(AtomicU64::new(1)),
            1,
        )
        .unwrap();
        let mut decoded = Decoder::new(input).unwrap();
        assert_eq!(decoded.sample_rate(), 16000);
        assert_eq!(decoded.channels(), 1);
        assert_eq!(decoded.by_ref().count(), 16000);
        decoded.try_seek(Duration::from_millis(500)).unwrap();
        assert_eq!(decoded.count(), 8000);
        server.join().unwrap();
    }
    #[test]
    fn cancellation_interrupts_reads_waiting_for_network() {
        let generation = Arc::new(AtomicU64::new(2));
        let mut input = Progressive {
            shared: Arc::new((
                Mutex::new(Buffer {
                    bytes: vec![],
                    done: false,
                    error: None,
                }),
                Condvar::new(),
            )),
            pos: 0,
            length: Some(100),
            generation,
            ticket: 1,
        };
        assert_eq!(
            input.read(&mut [0_u8; 4]).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(input.seek(SeekFrom::Current(-1)).is_err());
    }
    #[test]
    fn equalizer_zero_gain_is_transparent() {
        let source = rodio::buffer::SamplesBuffer::new(2, 48000, vec![0.1_f32; 10000]);
        let output: Vec<_> = Equalizer::new(source, Arc::new(RwLock::new([0.0; 10]))).collect();
        assert!(output.iter().all(|x| (*x - 0.1).abs() < 1e-5));
    }
    #[test]
    fn filters_are_stable_at_supported_rates() {
        for rate in [22050.0, 44100.0, 48000.0, 96000.0] {
            for hz in BANDS {
                for gain in [-12.0, 12.0] {
                    let f = Filter::peak(hz, gain, rate);
                    let mut state = [0.0; 2];
                    for i in 0..10000 {
                        let y = f.run(if i == 0 { 1.0 } else { 0.0 }, &mut state);
                        assert!(y.is_finite() && y.abs() < 8.0);
                    }
                }
            }
        }
    }
}
