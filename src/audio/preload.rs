use super::*;

type Ready = Result<(Decoded, SharedBuffer), String>;
type Completion = Arc<(Mutex<Option<Ready>>, Condvar)>;

/// One upcoming recording, with its own cancellation lifetime. Promoting it to
/// playback consumes the prepared decoder, never starts another HTTP request.
pub(super) struct Warm {
    pub key: String,
    pub cancel: Arc<AtomicU64>,
    result: Completion,
}
impl Warm {
    pub fn complete(&self) -> bool {
        self.result.0.lock().unwrap().as_ref().is_some_and(|r| {
            r.as_ref().is_ok_and(|(_, buffer)| {
                let data = buffer.0.lock().unwrap();
                data.done && data.error.is_none()
            })
        })
    }
    pub fn start(track: Track, services: Services) -> Self {
        let key = track.key();
        Self::spawn(key, move |cancel| prepare(&track, &services, cancel, 1))
    }
    fn spawn(key: String, load: impl FnOnce(Arc<AtomicU64>) -> Ready + Send + 'static) -> Self {
        let cancel = Arc::new(AtomicU64::new(1));
        let result: Completion = Arc::new((Mutex::new(None), Condvar::new()));
        let worker = result.clone();
        let token = cancel.clone();
        std::thread::spawn(move || {
            let ready = load(token);
            *worker.0.lock().unwrap() = Some(ready);
            worker.1.notify_all();
        });
        Self {
            key,
            cancel,
            result,
        }
    }
    pub fn take(self, generation: &AtomicU64, ticket: u64) -> Ready {
        let mut ready = self.result.0.lock().unwrap();
        loop {
            if generation.load(Ordering::Relaxed) != ticket {
                self.cancel.store(2, Ordering::Relaxed);
                return Err("Загрузка отменена".into());
            }
            if let Some(result) = ready.take() {
                if let Ok((_, buffer)) = &result {
                    if let Some(error) = &buffer.0.lock().unwrap().error {
                        return Err(format!("Предзагрузка аудио: {error}"));
                    }
                }
                return result;
            }
            ready = self
                .result
                .1
                .wait_timeout(ready, Duration::from_millis(40))
                .unwrap()
                .0;
        }
    }
}

pub(super) fn prepare(
    track: &Track,
    services: &Services,
    generation: Arc<AtomicU64>,
    ticket: u64,
) -> Ready {
    let mut last = String::new();
    for attempt in 0..3 {
        if generation.load(Ordering::Relaxed) != ticket {
            return Err("Загрузка отменена".into());
        }
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(150 * attempt));
        }
        let result = (|| -> Result<_> {
            // Signed stream URLs are obtained again on retry, rather than reusing
            // a stale URL after a 403 response.
            let url = services.resolve(track).context("Получение адреса аудио")?;
            if generation.load(Ordering::Relaxed) != ticket {
                bail!("Загрузка отменена");
            }
            let token = if track.provider == Provider::SoundCloud {
                services.soundcloud.clone()
            } else {
                String::new()
            };
            let input = Progressive::open(&services.http, &url, token, generation.clone(), ticket)
                .context("Открытие аудиопотока")?;
            let buffer = input.shared.clone();
            Ok((Decoder::new(input).context("Декодирование аудио")?, buffer))
        })();
        match result {
            Ok(ready) => return Ok(ready),
            Err(e) => {
                last = format!(
                    "{} · {} — {}: {e:#}",
                    track.provider.label(),
                    track.artist,
                    track.title
                )
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Downloads and decodes a real track using the configured Yandex account"]
    fn live_yandex_preload_decodes_audio_before_playback() {
        let services = Services::new(crate::store::secret("yandex"), String::new(), String::new());
        assert!(
            !services.yandex.is_empty(),
            "Yandex account is not configured"
        );
        let tracks = services.yandex_search("Massive Attack Teardrop").unwrap();
        let track = tracks.first().unwrap().clone();
        let warm = Warm::start(track, services);
        let (mut decoder, buffer) = warm.take(&AtomicU64::new(9), 9).unwrap();
        assert!(decoder.next().is_some());
        assert!(!buffer.0.lock().unwrap().bytes.is_empty());
        println!("Real Yandex audio prepared and decoded without opening an audio output");
    }
    #[test]
    fn promoting_a_warm_track_uses_the_existing_preparation() {
        let count = Arc::new(AtomicU64::new(0));
        let calls = count.clone();
        let warm = Warm::spawn("fixture".into(), move |_| {
            calls.fetch_add(1, Ordering::Relaxed);
            Err("fixture result".into())
        });
        let generation = AtomicU64::new(7);
        assert_eq!(warm.take(&generation, 7).err().unwrap(), "fixture result");
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn switching_away_cancels_a_pending_warm_track() {
        let warm = Warm::spawn("fixture".into(), move |_| {
            std::thread::sleep(Duration::from_millis(80));
            Err("late".into())
        });
        let token = warm.cancel.clone();
        assert!(warm.take(&AtomicU64::new(2), 1).is_err());
        assert_eq!(token.load(Ordering::Relaxed), 2);
    }
}
