use super::*;
use std::{sync::mpsc::Receiver, time::Instant};
struct Outgoing {
    sink: Sink,
    cancel: Option<Arc<AtomicU64>>,
    elapsed: Duration,
    last: Instant,
    duration: f32,
}

fn cancel(token: Option<Arc<AtomicU64>>) {
    if let Some(token) = token {
        token.store(2, Ordering::Relaxed);
    }
}
pub(super) fn run(
    rx: Receiver<Command>,
    shared: Arc<Mutex<Status>>,
    gen: Arc<AtomicU64>,
    eq: Arc<RwLock<[f32; 10]>>,
    preamp: Arc<RwLock<f32>>,
    results: Sender<Command>,
    initial_volume: f32,
) {
    let (_output, handle) = match OutputStream::try_default() {
        Ok(output) => output,
        Err(e) => {
            shared.lock().unwrap().error = Some(format!("Аудиовыход: {e}"));
            return;
        }
    };
    let mut sink = match Sink::try_new(&handle) {
        Ok(s) => s,
        Err(e) => {
            shared.lock().unwrap().error = Some(format!("Плеер: {e}"));
            return;
        }
    };
    let mut volume = initial_volume;
    sink.set_volume(volume);
    let mut active = false;
    let mut desired_playing = true;
    let mut download = None::<SharedBuffer>;
    let mut active_cancel = None;
    let mut pending = vec![];
    let mut gain = ramp::control(1., 0.);
    let mut outgoing: Option<Outgoing> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Command::Play(ticket, track, services, warm, fade)) => {
                desired_playing = true;
                for token in pending.drain(..) {
                    cancel(Some(token));
                }
                if let Some(old) = outgoing.take() {
                    old.sink.stop();
                    cancel(old.cancel);
                }
                if fade <= 0. || !active {
                    sink.stop();
                    cancel(active_cancel.take());
                    active = false;
                    download = None;
                    *shared.lock().unwrap() = Status {
                        ticket,
                        loading: true,
                        track: Some(track.clone()),
                        ..Default::default()
                    };
                } else {
                    shared.lock().unwrap().loading = true;
                }
                let token = Arc::new(AtomicU64::new(1));
                pending.push(token.clone());
                if let Some(warm) = &warm {
                    pending.push(warm.cancel.clone());
                }
                let tx = results.clone();
                let generation = gen.clone();
                std::thread::spawn(move || {
                    let mut selected = warm
                        .as_ref()
                        .map(|w| w.cancel.clone())
                        .unwrap_or_else(|| token.clone());
                    let result = if let Some(warm) = warm {
                        match warm.take(&generation, ticket) {
                            Ok(ready) => Ok(ready),
                            Err(_) if generation.load(Ordering::Relaxed) == ticket => {
                                selected.store(2, Ordering::Relaxed);
                                selected = token.clone();
                                preload::prepare(&track, &services, token, 1)
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        preload::prepare(&track, &services, token, 1)
                    };
                    let event = match result {
                        Ok((decoder, buffer)) => Command::Ready(
                            ticket,
                            track,
                            Box::new(decoder),
                            buffer,
                            Some(selected),
                            fade,
                        ),
                        Err(e) => Command::Failed(ticket, e),
                    };
                    let _ = tx.send(event);
                });
            }
            Ok(Command::Ready(ticket, track, decoder, buffer, token, fade))
                if gen.load(Ordering::Relaxed) == ticket =>
            {
                pending.clear();
                let new = match Sink::try_new(&handle) {
                    Ok(s) => s,
                    Err(e) => {
                        cancel(token);
                        shared.lock().unwrap().error = Some(format!("Плеер: {e}"));
                        continue;
                    }
                };
                let fade = if active && !sink.empty() {
                    fade.clamp(0., 12.)
                } else {
                    0.
                };
                if fade > 0. {
                    ramp::set(&gain, 0., fade);
                    outgoing = Some(Outgoing {
                        sink: std::mem::replace(&mut sink, new),
                        cancel: active_cancel.take(),
                        elapsed: Duration::ZERO,
                        last: Instant::now(),
                        duration: fade,
                    });
                } else {
                    sink.stop();
                    cancel(active_cancel.take());
                    sink = new;
                }
                let seconds = decoder
                    .total_duration()
                    .map(|d| d.as_secs_f32())
                    .unwrap_or(track.duration as f32);
                gain = ramp::control(1., fade);
                sink.set_volume(volume);
                sink.append(ramp::Smooth::new(
                    Equalizer::with_preamp(
                        decoder.convert_samples::<f32>(),
                        eq.clone(),
                        preamp.clone(),
                    ),
                    gain.clone(),
                    if fade > 0. { 0. } else { 1. },
                ));
                active = true;
                if !desired_playing {
                    sink.pause();
                    if let Some(old) = &outgoing {
                        old.sink.pause();
                    }
                }
                download = Some(buffer);
                active_cancel = token;
                *shared.lock().unwrap() = Status {
                    ticket,
                    playing: desired_playing,
                    duration: seconds,
                    track: Some(track),
                    ..Default::default()
                };
            }
            Ok(Command::Ready(_, _, _, _, token, _)) => cancel(token),
            Ok(Command::Failed(ticket, error)) if gen.load(Ordering::Relaxed) == ticket => {
                let mut state = shared.lock().unwrap();
                state.loading = false;
                if !active {
                    state.playing = false;
                }
                state.error = Some(error);
            }
            Ok(Command::Pause) => {
                if active {
                    desired_playing = sink.is_paused();
                    if sink.is_paused() {
                        sink.play();
                        if let Some(old) = &outgoing {
                            old.sink.play();
                        }
                    } else {
                        sink.pause();
                        if let Some(old) = &outgoing {
                            old.sink.pause();
                        }
                    }
                }
            }
            Ok(Command::SetPlaying(playing)) => {
                desired_playing = playing;
                if playing {
                    sink.play();
                    if let Some(old) = &outgoing {
                        old.sink.play();
                    }
                } else {
                    sink.pause();
                    if let Some(old) = &outgoing {
                        old.sink.pause();
                    }
                }
            }
            Ok(Command::Seek(seconds)) => {
                if let Some(old) = outgoing.take() {
                    old.sink.stop();
                    cancel(old.cancel);
                }
                ramp::set(&gain, 1., 0.);
                if let Err(e) = sink.try_seek(Duration::from_secs_f32(seconds.max(0.))) {
                    shared.lock().unwrap().error = Some(format!("Перемотка: {e}"));
                }
            }
            Ok(Command::Volume(v)) => {
                volume = v.clamp(0., 1.);
                sink.set_volume(volume);
                if let Some(old) = &outgoing {
                    old.sink.set_volume(volume);
                }
            }
            Ok(Command::Stop) => {
                for token in pending.drain(..) {
                    cancel(Some(token));
                }
                if let Some(old) = outgoing.take() {
                    old.sink.stop();
                    cancel(old.cancel);
                }
                cancel(active_cancel.take());
                sink.stop();
                active = false;
                download = None;
                *shared.lock().unwrap() = Status::default();
            }
            Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                for token in pending.drain(..) {
                    cancel(Some(token));
                }
                cancel(active_cancel.take());
                if let Some(old) = outgoing.take() {
                    old.sink.stop();
                    cancel(old.cancel);
                }
                sink.stop();
                break;
            }
            _ => {}
        }
        if let Some(old) = &mut outgoing {
            let now = Instant::now();
            if !sink.is_paused() {
                old.elapsed += now - old.last;
            }
            old.last = now;
            if old.elapsed.as_secs_f32() >= old.duration || old.sink.empty() {
                let old = outgoing.take().unwrap();
                old.sink.stop();
                cancel(old.cancel);
            }
        }
        if active {
            if let Some(error) = download
                .as_ref()
                .and_then(|b| b.0.lock().unwrap().error.clone())
            {
                sink.stop();
                active = false;
                let mut state = shared.lock().unwrap();
                state.playing = false;
                state.loading = false;
                state.error = Some(format!("Аудиопоток: {error}"));
                continue;
            }
            let mut state = shared.lock().unwrap();
            state.position = sink.get_pos().as_secs_f32();
            state.playing = !sink.is_paused() && !sink.empty();
            if sink.empty() {
                state.ended = true;
                active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready(ticket: u64, fade: f32) -> (Command, Arc<AtomicU64>) {
        let count = 48000_u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + count * 2).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(16000_u32.to_le_bytes());
        bytes.extend(32000_u32.to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((count * 2).to_le_bytes());
        bytes.resize(44 + count as usize * 2, 0);
        let token = Arc::new(AtomicU64::new(1));
        let length = bytes.len() as u64;
        let buffer = Arc::new((
            Mutex::new(Buffer {
                bytes,
                done: true,
                error: None,
            }),
            Condvar::new(),
        ));
        let decoder = Decoder::new(Progressive {
            shared: buffer.clone(),
            pos: 0,
            length: Some(length),
            generation: token.clone(),
            ticket: 1,
        })
        .unwrap();
        let track = crate::model::tests::track(&format!("Fixture {ticket}"), "Audio test", 3);
        (
            Command::Ready(
                ticket,
                track,
                Box::new(decoder),
                buffer,
                Some(token.clone()),
                fade,
            ),
            token,
        )
    }
    fn wait(state: &Arc<Mutex<Status>>, predicate: impl Fn(&Status) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate(&state.lock().unwrap()) {
            assert!(Instant::now() < deadline, "Audio transition timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "Opens the local audio device at zero volume to check overlapping sinks"]
    fn muted_native_crossfade_keeps_outgoing_audio_until_the_fade_finishes() {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Status::default()));
        let generation = Arc::new(AtomicU64::new(1));
        let worker_state = state.clone();
        let worker_generation = generation.clone();
        let results = tx.clone();
        let worker = std::thread::spawn(move || {
            run(
                rx,
                worker_state,
                worker_generation,
                Arc::new(RwLock::new([0.; 10])),
                Arc::new(RwLock::new(0.)),
                results,
                0.,
            )
        });
        let (first, first_cancel) = ready(1, 0.);
        tx.send(first).unwrap();
        wait(&state, |s| s.ticket == 1 && s.playing);
        generation.store(2, Ordering::Relaxed);
        let (second, _) = ready(2, 0.5);
        tx.send(second).unwrap();
        wait(&state, |s| s.ticket == 2 && s.playing);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(
            first_cancel.load(Ordering::Relaxed),
            1,
            "Previous recording was cancelled before its fade"
        );
        tx.send(Command::Pause).unwrap();
        wait(&state, |s| !s.playing);
        std::thread::sleep(Duration::from_millis(550));
        assert_eq!(
            first_cancel.load(Ordering::Relaxed),
            1,
            "Paused crossfade must not expire"
        );
        tx.send(Command::Pause).unwrap();
        wait(&state, |s| s.playing);
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(first_cancel.load(Ordering::Relaxed), 2);
        tx.send(Command::Seek(1.)).unwrap();
        wait(&state, |s| s.position >= 1.);
        tx.send(Command::Stop).unwrap();
        wait(&state, |s| s.track.is_none());
        tx.send(Command::Quit).unwrap();
        worker.join().unwrap();
    }
}
