use crate::{
    audio::{self, Player},
    glass::{GlassUi, Intent, Model, Page},
    model::{Edition, Provider, Track},
    providers::{yandex::Playlist, SearchResult, Services},
    recognition,
    store::{self, Settings, Store, ThemePreference},
};
use eframe::egui::{self, Color32, FontId, Vec2};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::Instant,
};

const CARD: Color32 = Color32::from_rgb(241, 240, 238);
const ACCENT: Color32 = Color32::from_rgb(232, 255, 163);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Sonora,
    Search,
    Library,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackSource {
    Sonora,
    Collection,
    Single,
}

enum Event {
    ConnectionChecked(Provider, u64, String, Result<String, String>),
    Artist(u64, u32, Result<crate::model::ArtistPage, String>),
    Lyrics(String, Result<Option<crate::lyrics::Lyrics>, String>),
    Search(u64, SearchResult, bool),
    Recommendations(u64, Result<Vec<Track>, String>, bool),
    Resolved(u64, Result<Track, String>),
    Prefetched(String, Result<Track, String>),
    Library(u64, Result<(Vec<Track>, Vec<Playlist>, String), String>),
    Playlist(u64, Result<Vec<Track>, String>),
    Alternatives(Track, SearchResult),
    Recognition(u64, Result<Track, String>),
}
enum Action {
    Play(Track),
    Like(Track),
    Radio(Track),
    Versions(Track),
    Open(String),
}

pub struct App {
    updates: crate::updates::Updates,
    cloud: crate::cloud::Cloud,
    sonora_ready: bool,
    start_after_library: bool,
    artists: Vec<crate::model::Artist>,
    artist: Option<crate::model::ArtistCatalog>,
    artist_request: u64,
    lyrics: Option<crate::lyrics::Lyrics>,
    lyrics_key: Option<String>,
    lyrics_loading: bool,
    lyrics_cache: HashMap<String, Option<crate::lyrics::Lyrics>>,
    crossfade_ticket: Option<u64>,
    tray: Option<crate::tray::Tray>,
    tray_attempted: bool,
    hidden: bool,
    quitting: bool,
    zen_request: Option<bool>,
    zen_previous_fullscreen: Option<bool>,
    playback_source: PlaybackSource,
    prefetch_requested: HashSet<String>,
    next_audio_key: Option<String>,
    error_generation: u64,
    interface: GlassUi,
    preview_status: Option<audio::Status>,
    queue_order: Option<Vec<String>>,
    store: Option<Store>,
    settings: Settings,
    player: Player,
    services: Services,
    tab: Tab,
    favorites: Vec<Track>,
    results: Vec<Track>,
    notices: Vec<String>,
    queue: Vec<Track>,
    cursor: Option<usize>,
    past: HashSet<String>,
    playlists: Vec<Playlist>,
    playlist_tracks: Vec<Track>,
    playlist_title: String,
    query: String,
    all_sources: bool,
    searching: bool,
    loading_library: bool,
    library_request: u64,
    loading_playlist: bool,
    generating: bool,
    resolving: bool,
    alternatives: Option<(Track, Vec<Track>, Vec<String>)>,
    versions_loading: bool,
    yandex_token: String,
    lastfm_key: String,
    soundcloud_token: String,
    account: String,
    message: String,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    search_id: u64,
    radio_id: u64,
    play_id: u64,
    recognition_id: u64,
    recording: Option<(Instant, Arc<AtomicBool>)>,
    handled_end: u64,
    recorded_ticket: u64,
    last_error: String,
    refill_cursor: Option<usize>,
    playlist_request: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_context(&cc.egui_ctx, std::env::var("WEDEX_PREVIEW").ok())
    }
    fn with_context(ctx: &egui::Context, preview: Option<String>) -> Self {
        crate::branding::configure(ctx);
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = Color32::from_rgb(249, 248, 246);
        visuals.window_fill = Color32::WHITE;
        visuals.extreme_bg_color = Color32::from_rgb(241, 240, 238);
        visuals.faint_bg_color = Color32::from_gray(242);
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke.color = Color32::BLACK;
        visuals.widgets.inactive.bg_fill = CARD;
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(230, 229, 226);
        visuals.widgets.active.bg_fill = ACCENT;
        visuals.override_text_color = Some(Color32::from_rgb(28, 26, 30));
        ctx.set_visuals(visuals);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = Vec2::new(12.0, 10.0);
            s.spacing.button_padding = Vec2::new(14.0, 9.0);
            s.text_styles
                .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        });
        egui_extras::install_image_loaders(ctx);
        let (store, message) = if preview.is_some() {
            (None, String::new())
        } else {
            match Store::open() {
                Ok(s) => (Some(s), String::new()),
                Err(e) => (None, format!("Не удалось открыть медиатеку: {e}")),
            }
        };
        let mut settings = store.as_ref().map(Store::settings).unwrap_or_default();
        settings.eq = settings.eq.map(|gain| gain.clamp(-12., 12.));
        settings.eq_preamp = settings.eq_preamp.clamp(-12., 12.);
        settings.crossfade = settings.crossfade.clamp(0., 12.);
        if !crate::equalizer::PRESETS.iter().any(|preset| {
            preset.name == settings.eq_preset
                && preset.gains == settings.eq
                && settings.eq_preamp == 0.
        }) {
            settings.eq_preset = "Своя настройка".into();
        }
        let favorites = store
            .as_ref()
            .and_then(|s| s.favorites().ok())
            .unwrap_or_default();
        let yandex_token = if preview.is_some() {
            String::new()
        } else {
            store::secret("yandex")
        };
        let lastfm_key = if preview.is_some() {
            String::new()
        } else {
            store::secret("lastfm")
        };
        let soundcloud_token = if preview.is_some() {
            String::new()
        } else {
            store::secret("soundcloud")
        };
        let mut services = Services::new(
            yandex_token.clone(),
            lastfm_key.clone(),
            soundcloud_token.clone(),
        );
        services.hitmo_base = crate::providers::connections::hitmo_origin(&settings.hitmo_base)
            .unwrap_or_else(|_| crate::providers::connections::DEFAULT_HITMO.into());
        #[cfg(not(test))]
        let player = Player::new(settings.volume, settings.eq);
        #[cfg(test)]
        let player = if preview.is_some() {
            Player::for_tests(settings.eq)
        } else {
            Player::new(settings.volume, settings.eq)
        };
        *player.preamp.write().unwrap() = settings.eq_preamp.clamp(-12., 12.);
        let (tx, rx) = mpsc::channel();
        let past = store
            .as_ref()
            .and_then(|s| s.history().ok())
            .unwrap_or_default()
            .iter()
            .map(Track::identity)
            .collect();
        let mut interface = GlassUi::new();
        interface.yandex = yandex_token.clone();
        interface.lastfm = lastfm_key.clone();
        interface.soundcloud = soundcloud_token.clone();
        if let Some(page) = &preview {
            interface.preview_page(page);
        }
        let mut app = Self {
            updates: Default::default(),
            cloud: crate::cloud::Cloud::new(preview.is_some()),
            sonora_ready: false,
            start_after_library: false,
            artists: Vec::new(),
            artist: None,
            artist_request: 0,
            lyrics: None,
            lyrics_key: None,
            lyrics_loading: false,
            lyrics_cache: HashMap::new(),
            crossfade_ticket: None,
            tray: None,
            tray_attempted: false,
            hidden: false,
            quitting: false,
            zen_request: None,
            zen_previous_fullscreen: None,
            playback_source: PlaybackSource::Single,
            prefetch_requested: HashSet::new(),
            next_audio_key: None,
            error_generation: 0,
            interface,
            preview_status: None,
            queue_order: None,
            store,
            settings,
            player,
            services,
            tab: Tab::Sonora,
            favorites,
            results: vec![],
            notices: vec![],
            queue: vec![],
            cursor: None,
            past,
            playlists: vec![],
            playlist_tracks: vec![],
            playlist_title: String::new(),
            query: String::new(),
            all_sources: true,
            searching: false,
            loading_library: false,
            library_request: 0,
            loading_playlist: false,
            generating: false,
            resolving: false,
            alternatives: None,
            versions_loading: false,
            yandex_token,
            lastfm_key,
            soundcloud_token,
            account: String::new(),
            message,
            tx,
            rx,
            search_id: 0,
            radio_id: 0,
            play_id: 0,
            recognition_id: 0,
            recording: None,
            handled_end: 0,
            recorded_ticket: 0,
            last_error: String::new(),
            refill_cursor: None,
            playlist_request: 0,
        };
        if !app.message.is_empty() {
            let error = app.message.clone();
            app.report_error("startup", error);
        }
        if let Some(page) = preview {
            app.install_preview(&page);
        } else if !app.services.yandex.is_empty() {
            app.sync();
        }
        app
    }
    fn job(&self, work: impl FnOnce(Services) -> Event + Send + 'static) {
        let tx = self.tx.clone();
        let services = self.services.clone();
        std::thread::spawn(move || {
            let event = work(services);
            let _ = tx.send(event);
        });
    }
    fn persist(&mut self) {
        if let Some(store) = &self.store {
            if let Err(e) = store.save_settings(&self.settings) {
                self.report_error("storage", e.to_string());
            }
        }
    }
    fn report_error(&mut self, context: &str, error: impl AsRef<str>) {
        let error = error.as_ref();
        let _ = crate::diagnostics::append(
            &crate::diagnostics::log_path(),
            context,
            error,
            &[&self.yandex_token, &self.lastfm_key, &self.soundcloud_token],
        );
        self.error_generation = self.error_generation.wrapping_add(1);
        self.message = crate::diagnostics::USER_ERROR.into();
    }
    fn search(&mut self) {
        if self.query.trim().is_empty() {
            return;
        }
        self.search_id += 1;
        let id = self.search_id;
        let query = self.query.trim().to_owned();
        let all = self.all_sources;
        self.searching = true;
        self.results.clear();
        self.artists.clear();
        self.artist_request = self.artist_request.wrapping_add(1);
        self.artist = None;
        self.notices.clear();
        let tx = self.tx.clone();
        self.job(move |s| {
            s.search_progress(&query, all, |batch| {
                let _ = tx.send(Event::Search(id, batch, false));
            });
            Event::Search(id, SearchResult::default(), true)
        });
    }
    fn load_artist_page(&mut self) {
        let Some(catalog) = &mut self.artist else {
            return;
        };
        let Some(page) = catalog.next_page else {
            return;
        };
        if catalog.loading {
            return;
        }
        catalog.loading = true;
        catalog.failed = false;
        let artist_id = catalog.artist.id.clone();
        let request = self.artist_request;
        self.job(move |services| {
            Event::Artist(
                request,
                page,
                services
                    .yandex_artist_page(&artist_id, page)
                    .map_err(|e| e.to_string()),
            )
        });
    }
    fn save_connection(&mut self, provider: Provider, value: &str) {
        self.save_connection_impl(provider, value, true);
    }
    fn save_connection_impl(&mut self, provider: Provider, value: &str, notify: bool) {
        let value = value.trim();
        if self.preview_status.is_some() {
            self.interface.connection_saved(provider, value);
            return;
        }
        let saved = (|| -> anyhow::Result<String> {
            let key = match provider {
                Provider::Yandex => "yandex",
                Provider::LastFm => "lastfm",
                Provider::SoundCloud => "soundcloud",
                Provider::Hitmotop => return crate::providers::connections::hitmo_origin(value),
                Provider::YouTube => return Ok(String::new()),
            };
            store::save_secret(key, value)?;
            Ok(value.to_owned())
        })();
        match saved {
            Ok(value) => {
                if notify {
                    let name = match provider {
                        Provider::Yandex => "yandex",
                        Provider::LastFm => "lastfm",
                        Provider::SoundCloud => "soundcloud",
                        Provider::Hitmotop => "hitmo",
                        Provider::YouTube => "",
                    };
                    if !name.is_empty() {
                        self.cloud.connection_edited(name, &value);
                    }
                }
                match provider {
                    Provider::Yandex => {
                        self.library_request = self.library_request.wrapping_add(1);
                        self.loading_library = false;
                        self.yandex_token = value.clone();
                        self.services.yandex = value.clone();
                        if value.is_empty() {
                            self.account.clear();
                        }
                    }
                    Provider::LastFm => {
                        self.lastfm_key = value.clone();
                        self.services.lastfm = value.clone();
                    }
                    Provider::SoundCloud => {
                        self.soundcloud_token = value.clone();
                        self.services.soundcloud = value.clone();
                    }
                    Provider::Hitmotop => {
                        self.settings.hitmo_base = value.clone();
                        self.services.hitmo_base = value.clone();
                        self.persist();
                    }
                    Provider::YouTube => {}
                }
                self.interface.connection_saved(provider, &value);
                if provider == Provider::Yandex && !value.is_empty() {
                    self.sync();
                }
            }
            Err(error) => {
                let _ = crate::diagnostics::append(
                    &crate::diagnostics::log_path(),
                    "connection-save",
                    &error.to_string(),
                    &[
                        value,
                        &self.yandex_token,
                        &self.lastfm_key,
                        &self.soundcloud_token,
                    ],
                );
                self.interface.connection_save_failed();
                self.message = crate::diagnostics::USER_ERROR.into();
                self.error_generation = self.error_generation.wrapping_add(1);
            }
        }
    }
    fn fetch_radio(&mut self, seeds: Vec<Track>, autoplay: bool) {
        if self.generating {
            return;
        }
        self.radio_id += 1;
        let id = self.radio_id;
        self.generating = true;
        let mut excluded = self.past.clone();
        excluded.extend(self.queue.iter().map(Track::identity));
        let discovery = self.settings.discovery;
        let recent = self
            .settings
            .recent_recommendations
            .iter()
            .cloned()
            .collect();
        self.job(move |s| {
            Event::Recommendations(
                id,
                s.recommendations(&seeds, &excluded, discovery, &recent)
                    .map_err(|e| e.to_string()),
                autoplay,
            )
        });
    }
    fn launch_radio(&mut self) {
        self.start_after_library = false;
        let mut seeds = vec![];
        if !self.settings.last_artist.trim().is_empty()
            && !self.settings.last_title.trim().is_empty()
        {
            seeds.push(Track {
                id: "manual-seed".into(),
                provider: Provider::LastFm,
                title: self.settings.last_title.trim().into(),
                artist: self.settings.last_artist.trim().into(),
                album: String::new(),
                duration: 0,
                cover: None,
                page_url: String::new(),
                edition: Edition::Unknown,
                reason: String::new(),
                score: 0.0,
            });
        } else {
            seeds = crate::discovery::Sampler::fresh()
                .seeds(&self.favorites, &self.settings.recent_radio_seeds);
        }
        if seeds.is_empty() {
            self.message =
                "Введите исполнителя и трек для старта или импортируйте понравившиеся из Яндекса"
                    .into();
            return;
        }
        self.sonora_ready = false;
        self.generating = false;
        self.radio_id += 1;
        self.playback_source = PlaybackSource::Sonora;
        crate::discovery::remember(
            &mut self.settings.recent_recommendations,
            self.past.drain(),
            120,
        );
        crate::discovery::remember(
            &mut self.settings.recent_radio_seeds,
            seeds.iter().map(Track::identity),
            16,
        );
        self.player.stop();
        self.prefetch_requested.clear();
        self.next_audio_key = None;
        self.queue.clear();
        self.refill_cursor = None;
        self.cursor = None;
        self.play_id += 1;
        self.resolving = false;
        self.persist();
        self.fetch_radio(seeds, true);
    }
    fn reset_radio(&mut self) {
        self.settings.last_artist.clear();
        self.settings.last_title.clear();
        self.persist();
        self.tab = Tab::Sonora;
        if self.favorites.is_empty() && self.loading_library {
            self.start_after_library = true;
            return;
        }
        if self.favorites.is_empty() {
            self.message = "Добавьте треки в понравившиеся, чтобы запустить Sonora".into();
            return;
        }
        self.launch_radio();
    }
    fn play_collection(&mut self, tracks: Vec<Track>, index: usize) {
        if index >= tracks.len() {
            return;
        }
        self.start_after_library = false;
        self.radio_id += 1;
        self.generating = false;
        self.refill_cursor = None;
        self.playback_source = PlaybackSource::Collection;
        self.queue_order = None;
        self.queue = tracks;
        self.prefetch_requested.clear();
        self.next_audio_key = None;
        if self.preview_status.is_some() {
            self.glass_action(Intent::QueueIndex(index));
        } else {
            self.start(index);
        }
    }
    fn start(&mut self, index: usize) {
        let Some(mut track) = self.queue.get(index).cloned() else {
            return;
        };
        self.cursor = Some(index);
        self.play_id += 1;
        self.resolving = false;
        if self.settings.prefer_original {
            if let Some(replacement) = self.store.as_ref().and_then(|s| s.replacement(&track)) {
                track = replacement;
            }
        }
        if track.provider == Provider::LastFm {
            self.player.stop();
            self.resolving = true;
            let id = self.play_id;
            let original = self.settings.prefer_original;
            self.job(move |s| {
                Event::Resolved(
                    id,
                    s.resolve_recommendation(&track, original)
                        .map_err(|e| e.to_string()),
                )
            });
        } else {
            self.queue[index] = track.clone();
            self.player
                .transition(track, self.services.clone(), self.settings.crossfade);
        }
    }
    fn prepare_upcoming(&mut self) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let upcoming = self
            .queue
            .iter()
            .skip(cursor + 1)
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        for mut track in upcoming {
            if self.settings.prefer_original {
                if let Some(t) = self.store.as_ref().and_then(|s| s.replacement(&track)) {
                    track = t;
                }
            }
            if track.provider == Provider::LastFm && self.prefetch_requested.insert(track.key()) {
                let key = track.key();
                let original = self.settings.prefer_original;
                self.job(move |s| {
                    Event::Prefetched(
                        key,
                        s.resolve_recommendation(&track, original)
                            .map_err(|e| format!("{} — {}: {e:#}", track.artist, track.title)),
                    )
                });
            }
        }
        let next = self
            .queue
            .get(cursor + 1)
            .cloned()
            .map(|t| {
                if self.settings.prefer_original {
                    self.store
                        .as_ref()
                        .and_then(|s| s.replacement(&t))
                        .unwrap_or(t)
                } else {
                    t
                }
            })
            .filter(|t| t.provider != Provider::LastFm);
        let key = next.as_ref().map(Track::key);
        if key != self.next_audio_key {
            self.player.preload(next, self.services.clone());
            self.next_audio_key = key;
        }
    }
    fn next(&mut self) {
        let next = self.cursor.map(|i| i + 1).unwrap_or(0);
        if next < self.queue.len() {
            self.start(next);
        } else if self.playback_source == PlaybackSource::Sonora {
            if let Some(seed) = self.queue.last().cloned() {
                self.fetch_radio(vec![seed], true);
            }
        }
    }
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Play(track) => {
                self.start_after_library = false;
                self.radio_id += 1;
                self.generating = false;
                self.refill_cursor = None;
                self.playback_source = PlaybackSource::Single;
                self.queue_order = None;
                self.queue = vec![track];
                self.next_audio_key = None;
                self.prefetch_requested.clear();
                self.start(0);
            }
            Action::Like(track) => {
                let exists = self.favorites.iter().any(|t| t.key() == track.key());
                if let Some(store) = &self.store {
                    match store.favorite(&track, !exists) {
                        Ok(()) => {
                            if exists {
                                self.favorites.retain(|t| t.key() != track.key());
                            } else {
                                self.favorites.insert(0, track);
                            }
                        }
                        Err(e) => self.report_error("operation", e.to_string()),
                    }
                }
            }
            Action::Radio(track) => {
                self.start_after_library = false;
                self.sonora_ready = true;
                self.settings.last_artist = track.artist.clone();
                self.settings.last_title = track.title.clone();
                self.radio_id += 1;
                self.generating = false;
                self.refill_cursor = None;
                self.playback_source = PlaybackSource::Sonora;
                self.queue = vec![track];
                self.queue_order = None;
                self.prefetch_requested.clear();
                self.next_audio_key = None;
                self.persist();
                self.start(0);
            }
            Action::Versions(track) => {
                if !self.versions_loading {
                    self.versions_loading = true;
                    self.alternatives = Some((track.clone(), vec![], vec![]));
                    self.job(move |s| {
                        let result = s.alternatives(&track);
                        Event::Alternatives(track, result)
                    });
                }
            }
            Action::Open(url) => {
                if let Err(e) = crate::platform::open_url(&url) {
                    self.report_error("open-link", e.to_string());
                }
            }
        }
    }
    fn sync(&mut self) {
        if self.loading_library {
            return;
        }
        self.loading_library = true;
        self.library_request = self.library_request.wrapping_add(1);
        let id = self.library_request;
        self.job(move |s| Event::Library(id, s.yandex_library().map_err(|e| e.to_string())));
    }
    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::ConnectionChecked(provider, revision, value, result) => {
                    if self
                        .interface
                        .connection_result(provider, revision, result.clone())
                    {
                        if let Err(error) = result {
                            let _ = crate::diagnostics::append(
                                &crate::diagnostics::log_path(),
                                "connection-check",
                                &error,
                                &[
                                    &value,
                                    &self.yandex_token,
                                    &self.lastfm_key,
                                    &self.soundcloud_token,
                                ],
                            );
                        }
                    }
                }

                Event::Lyrics(key, result) => {
                    let value = match result {
                        Ok(value) => value,
                        Err(error) => {
                            self.report_error("lyrics", error);
                            None
                        }
                    };
                    if self.lyrics_cache.len() >= 40 {
                        self.lyrics_cache.clear();
                    }
                    self.lyrics_cache.insert(key.clone(), value.clone());
                    if self.lyrics_key.as_ref() == Some(&key) {
                        self.lyrics = value;
                        self.lyrics_loading = false;
                    }
                }
                Event::Prefetched(key, result) => {
                    let index = self
                        .queue
                        .iter()
                        .enumerate()
                        .find(|(i, t)| *i > self.cursor.unwrap_or(0) && t.key() == key)
                        .map(|(i, _)| i);
                    if let Some(index) = index {
                        match result {
                            Ok(track) => self.queue[index] = track,
                            Err(error) => {
                                let _ = crate::diagnostics::append(
                                    &crate::diagnostics::log_path(),
                                    "recommendation-prefetch",
                                    &error,
                                    &[&self.yandex_token, &self.lastfm_key, &self.soundcloud_token],
                                );
                                self.queue.remove(index);
                            }
                        }
                    }
                }
                Event::Artist(request, page, result) if request == self.artist_request => {
                    if let Some(catalog) = &mut self.artist {
                        catalog.loading = false;
                        match result {
                            Ok(result) => {
                                catalog.failed = false;
                                if let Some(artist) = result.artist {
                                    catalog.artist = artist;
                                }
                                catalog.total = result.total.or(catalog.total);
                                catalog.next_page = result.next_page.filter(|next| *next > page);
                                let mut seen: HashSet<_> =
                                    catalog.tracks.iter().map(Track::key).collect();
                                catalog.tracks.extend(
                                    result.tracks.into_iter().filter(|t| seen.insert(t.key())),
                                );
                                if !result.notices.is_empty() {
                                    self.report_error("artist-info", result.notices.join("\n"));
                                }
                            }
                            Err(error) => {
                                catalog.failed = true;
                                self.report_error("artist-tracks", error);
                            }
                        }
                    }
                }
                Event::Search(id, batch, complete) if id == self.search_id => {
                    self.searching = !complete;
                    let mut result = SearchResult {
                        tracks: std::mem::take(&mut self.results),
                        artists: std::mem::take(&mut self.artists),
                        ..Default::default()
                    };
                    result.merge(batch);
                    self.results = result.tracks;
                    self.artists = result.artists;
                    if !result.notices.is_empty() {
                        self.report_error("search", result.notices.join("\n"));
                    }
                    self.notices.clear();
                }
                Event::Recommendations(id, result, autoplay)
                    if id == self.radio_id && self.playback_source == PlaybackSource::Sonora =>
                {
                    self.generating = false;
                    match result {
                        Ok(tracks) => {
                            let index = self.queue.len();
                            if !tracks.is_empty() {
                                self.sonora_ready = true;
                            }
                            if tracks.is_empty() {
                                self.message="Last.fm пока не нашёл новых рекомендаций для этих треков. Попробуйте другой исходный трек".into();
                            }
                            crate::discovery::remember(
                                &mut self.settings.recent_recommendations,
                                tracks.iter().map(Track::identity),
                                120,
                            );
                            self.persist();
                            self.queue.extend(tracks);
                            let stalled = self.player.status().ended
                                && self.cursor.is_some_and(|i| i + 1 >= index);
                            if (autoplay || stalled) && index < self.queue.len() {
                                self.start(index);
                            }
                        }
                        Err(e) => self.report_error("recommendations", e),
                    }
                }
                Event::Resolved(id, result) if id == self.play_id => {
                    self.resolving = false;
                    match result {
                        Ok(mut track) => {
                            if self.settings.prefer_original {
                                if let Some(replacement) =
                                    self.store.as_ref().and_then(|s| s.replacement(&track))
                                {
                                    track = replacement;
                                }
                            }
                            if let Some(i) = self.cursor {
                                self.queue[i] = track.clone();
                            }
                            self.player.transition(
                                track,
                                self.services.clone(),
                                self.settings.crossfade,
                            );
                        }
                        Err(e) => {
                            self.report_error("resolve-recording", e);
                            if self.cursor.is_some_and(|i| i + 1 < self.queue.len()) {
                                self.next();
                            }
                        }
                    }
                }
                Event::Library(id, result) if id == self.library_request => {
                    self.loading_library = false;
                    let start_radio = std::mem::take(&mut self.start_after_library);
                    match result {
                        Ok((tracks, playlists, name)) => {
                            if let Some(store) = &self.store {
                                if let Err(e) = store.import_yandex(&tracks) {
                                    self.report_error("import", e.to_string());
                                    continue;
                                }
                                self.favorites =
                                    store.favorites().unwrap_or_else(|_| tracks.clone());
                            } else {
                                self.favorites = tracks.clone();
                            }
                            self.playlists = playlists;
                            self.account = name;
                            self.message = format!(
                                "Импортировано {} понравившихся треков и {} плейлистов",
                                tracks.len(),
                                self.playlists.len()
                            );
                            if start_radio {
                                self.launch_radio();
                            }
                        }
                        Err(e) => self.report_error("service", e),
                    }
                }
                Event::Playlist(id, result) if id == self.playlist_request => {
                    self.loading_playlist = false;
                    match result {
                        Ok(tracks) => self.playlist_tracks = tracks,
                        Err(e) => self.report_error("service", e),
                    }
                }
                Event::Alternatives(track, result) => {
                    self.versions_loading = false;
                    if self
                        .alternatives
                        .as_ref()
                        .is_some_and(|(current, _, _)| current.key() == track.key())
                    {
                        if !result.notices.is_empty() {
                            self.report_error("versions", result.notices.join("\n"));
                        }
                        self.alternatives = Some((track, result.tracks, Vec::new()));
                    }
                }
                Event::Recognition(id, result) if id == self.recognition_id => {
                    self.recording = None;
                    match result {
                        Ok(track) => {
                            self.interface.recognized = Some(track);
                        }
                        Err(e) => {
                            self.report_error("recognition", e);
                            self.interface.overlay = None;
                        }
                    }
                }
                _ => {}
            }
        }
        let status = self.player.status();
        if status.playing && status.ticket != self.recorded_ticket {
            self.recorded_ticket = status.ticket;
            if let Some(track) = &status.track {
                if self.playback_source == PlaybackSource::Sonora {
                    self.past.insert(track.identity());
                }
                if let Some(store) = &self.store {
                    if let Err(e) = store.played(track) {
                        self.report_error("history", e.to_string());
                    }
                }
            }
        }
        if let Some(error) = &status.error {
            let signature = format!("{}:{error}", status.ticket);
            if signature != self.last_error {
                self.report_error("playback", error);
                self.last_error = signature;
                if self.controlled_playback().is_none()
                    && self.cursor.is_some_and(|i| i + 1 < self.queue.len())
                {
                    self.next();
                }
            }
        }
        if !status.loading && !self.resolving && self.controlled_playback().is_none() {
            if status.ended && status.ticket != self.handled_end {
                self.handled_end = status.ticket;
                if self.settings.repeat_current {
                    if let Some(index) = self.cursor {
                        self.start(index);
                    }
                } else {
                    self.next();
                }
            } else if should_crossfade(&status, &self.settings, self.crossfade_ticket)
                && self
                    .cursor
                    .and_then(|i| self.queue.get(i + 1))
                    .is_some_and(|t| self.player.preloaded(&t.key()))
            {
                self.crossfade_ticket = Some(status.ticket);
                self.next();
            }
        }
        let displayed = self.display_status();
        self.prepare_lyrics(&displayed);
        if self.controlled_playback().is_none()
            && self.playback_source == PlaybackSource::Sonora
            && self
                .cursor
                .is_some_and(|i| self.queue.len().saturating_sub(i) < 6)
            && !self.generating
            && !self.queue.is_empty()
            && self.refill_cursor != self.cursor
        {
            self.refill_cursor = self.cursor;
            let seeds = self
                .cursor
                .and_then(|i| self.queue.get(i))
                .cloned()
                .into_iter()
                .collect();
            self.fetch_radio(seeds, false);
        }
        if self.controlled_playback().is_none() {
            self.prepare_upcoming();
        }
    }
    fn prepare_lyrics(&mut self, status: &audio::Status) {
        if !self.settings.lyrics_enabled {
            return;
        }
        let Some(track) = status
            .track
            .as_ref()
            .filter(|t| t.provider != Provider::LastFm)
        else {
            return;
        };
        let key = track.key();
        if self.lyrics_key.as_ref() == Some(&key) {
            return;
        }
        self.lyrics_key = Some(key.clone());
        self.lyrics = None;
        self.lyrics_loading = false;
        if let Some(cached) = self.lyrics_cache.get(&key) {
            self.lyrics = cached.clone();
            return;
        }
        self.lyrics_loading = true;
        let track = track.clone();
        self.job(move |services| {
            Event::Lyrics(
                key,
                crate::lyrics::fetch(&services, &track).map_err(|e| e.to_string()),
            )
        });
    }
    fn record(&mut self) {
        if let Some((_, cancel)) = self.recording.take() {
            cancel.store(true, Ordering::Relaxed);
            self.recognition_id += 1;
            self.message = "Распознавание отменено".into();
            return;
        }
        if self.player.status().playing {
            self.player.pause();
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.recording = Some((Instant::now(), cancelled.clone()));
        self.recognition_id += 1;
        let id = self.recognition_id;
        self.job(move |s| {
            Event::Recognition(
                id,
                recognition::recognize(&s, cancelled).map_err(|e| e.to_string()),
            )
        });
    }
    fn install_preview(&mut self, page: &str) {
        self.sonora_ready = page != "sonora-start";
        let mut tracks = crate::glass::demo_tracks();
        for (i, t) in tracks.iter_mut().enumerate() {
            t.provider = match i {
                2 => Provider::SoundCloud,
                3 => Provider::Hitmotop,
                _ => Provider::Yandex,
            };
        }
        self.queue = tracks.clone();
        self.cursor = Some(0);
        self.favorites = tracks.clone();
        self.playlist_tracks = vec![
            tracks[5].clone(),
            tracks[3].clone(),
            tracks[4].clone(),
            tracks[0].clone(),
            tracks[1].clone(),
            tracks[2].clone(),
        ];
        self.playlist_title = "Ночной маршрут".into();
        self.playlists = [
            "Ночной маршрут",
            "На повторе",
            "Громче мыслей",
            "Медленный вечер",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, title)| Playlist {
            uid: "preview".into(),
            kind: i.to_string(),
            title: title.into(),
            count: 6,
            cover: None,
        })
        .collect();
        if page == "results" || page == "artist" {
            self.query = "The Weeknd".into();
            self.results = (0..30)
                .map(|i| {
                    let mut track = tracks[1].clone();
                    track.title = [
                        "Blinding Lights",
                        "Save Your Tears",
                        "Starboy",
                        "The Hills",
                        "After Hours",
                        "In Your Eyes",
                    ][i % 6]
                        .into();
                    track.id = format!("preview-{i}");
                    track.provider = [
                        Provider::Yandex,
                        Provider::Hitmotop,
                        Provider::SoundCloud,
                        Provider::YouTube,
                    ][i % 4];
                    track
                })
                .collect();
            let artist = crate::model::Artist {
                id: "37027".into(),
                name: "The Weeknd".into(),
                cover: Some("weeknd".into()),
                tracks: Some(124),
                listeners: Some(1682262),
            };
            self.artists = vec![artist.clone()];
            if page == "artist" {
                self.artist = Some(crate::model::ArtistCatalog {
                    artist,
                    tracks: self
                        .results
                        .iter()
                        .cloned()
                        .map(|mut t| {
                            t.artist = "The Weeknd".into();
                            t.provider = Provider::Yandex;
                            t
                        })
                        .collect(),
                    total: Some(124),
                    next_page: None,
                    loading: false,
                    failed: false,
                });
            }
        }
        if page == "eq-warm" {
            self.settings.eq = [4., 3., 2., 1., 0., -1., -1., 0., 1., 2.];
        }
        if page == "versions" {
            self.alternatives = Some((
                tracks[0].clone(),
                vec![tracks[0].clone(), tracks[3].clone(), tracks[2].clone()],
                vec![],
            ));
        }
        if page == "recognized" {
            self.interface.recognized = Some(tracks[1].clone());
        }
        self.settings.theme = if std::env::var_os("WEDEX_PREVIEW_DARK").is_some() {
            ThemePreference::Dark
        } else {
            ThemePreference::Light
        };
        self.settings.volume = 0.75;
        if page == "sonora-track" {
            self.settings.last_artist = "Massive Attack".into();
            self.settings.last_title = "Teardrop".into();
        }
        let current = if page == "playlist" { 5 } else { 0 };
        self.cursor = Some(current);
        self.preview_status = Some(audio::Status {
            track: Some(tracks[current].clone()),
            duration: tracks[current].duration as f32,
            position: 48.,
            playing: true,
            ..Default::default()
        });
        if page == "sonora-start" {
            self.queue.clear();
            self.cursor = None;
            self.preview_status = Some(audio::Status::default());
        }
        if matches!(page, "lyrics" | "zen" | "zen-lyrics") {
            self.settings.lyrics_enabled = page != "zen";
            self.interface.zen = page.starts_with("zen");
            self.lyrics = Some(crate::lyrics::Lyrics {
                lines: crate::lyrics::parse_lrc(
                    "[00:00.00]Город медленно просыпается
[00:24.00]Мы оставляем шум позади
[00:45.00]Музыка выбирает дорогу
[01:02.00]И новый день уже впереди
[01:20.00]Свет отражается в окнах",
                ),
                ..Default::default()
            });
            if page == "zen-lyrics" {
                self.lyrics.as_mut().unwrap().lines=crate::lyrics::parse_lrc("[00:00.00]Город медленно просыпается\n[00:06.00]Свет ложится на тихие улицы\n[00:12.00]Мы оставляем шум позади\n[00:24.00]Ветер открывает нам окна\n[00:45.00]Музыка выбирает дорогу\n[01:02.00]И новый день уже впереди\n[01:20.00]Свет отражается в окнах\n[01:32.00]Незнакомые улицы рядом\n[01:45.00]Мы продолжим идти за звуком");
            }
        }
        if page == "error" {
            self.report_error("preview-service", "HTTP 503: fixture service unavailable");
        }
        if page == "updates" {
            self.updates.view.message = "Доступно обновление".into();
            self.updates.view.app = Some(wedex_update::Release {
                version: "0.8.1".into(),
                notes: "Улучшения Sonora и воспроизведения.\nОбновление сохраняет вашу музыку и настройки.".into(),
                page: "https://github.com/AntonDevia/WeDex/releases".into(),
                url: String::new(),
                sha256: String::new(),
                size: 1,
                target: wedex_update::Target::Windows,
            });
        }
    }
    fn glass_model(&self) -> Model {
        let public = |text: &str| {
            let mut out = text.to_owned();
            for secret in [&self.yandex_token, &self.lastfm_key, &self.soundcloud_token] {
                if !secret.is_empty() {
                    out = out.replace(secret, "•••");
                }
            }
            out
        };
        let mut model = Model {
            player_liked: None,
            updates: self.updates.view.clone(),
            sonora_ready: self.sonora_ready,
            sonora_loading: self.generating || self.start_after_library,
            error_generation: self.error_generation,
            settings: self.settings.clone(),
            status: self
                .preview_status
                .clone()
                .unwrap_or_else(|| self.player.status()),
            queue: self.queue.clone(),
            cursor: self.cursor,
            favorites: self.favorites.clone(),
            results: self.results.clone(),
            artists: self.artists.clone(),
            artist: self.artist.clone(),
            playlists: self.playlists.clone(),
            playlist_tracks: self.playlist_tracks.clone(),
            playlist_title: self.playlist_title.clone(),
            query: self.query.clone(),
            searching: self.searching,
            loading_library: self.loading_library,
            loading_playlist: self.loading_playlist,
            resolving: self.resolving,
            recording: self.recording.is_some(),
            account: self.account.clone(),
            connected: if self.preview_status.is_some() {
                [true, true, false, true, true]
            } else {
                [
                    !self.services.yandex.is_empty(),
                    !self.lastfm_key.is_empty(),
                    !self.soundcloud_token.is_empty(),
                    true,
                    true,
                ]
            },
            lyrics: self.lyrics.clone(),
            lyrics_loading: self.lyrics_loading,
            alternatives: self.alternatives.clone(),
            versions_loading: self.versions_loading,
            message: public(&self.message),
            preview: self.preview_status.is_some(),
        };
        if let Some(p) = self.controlled_playback() {
            model.status = self.display_status();
            model.sonora_ready = p.track.is_some();
            model.sonora_loading = p.loading;
            model.queue = p
                .queue
                .iter()
                .filter_map(|t| crate::cloud::display_track(t).ok())
                .collect();
            model.cursor = p.cursor;
            model.player_liked = p.track.as_ref().map(|t| t.liked);
            model.settings.volume = p.volume;
            model.settings.repeat_current = p.repeat;
            model.settings.eq = p.eq;
            model.settings.eq_preamp = p.preamp;
            model.settings.eq_preset = crate::equalizer::PRESETS
                .iter()
                .find(|preset| preset.gains == p.eq && p.preamp == 0.)
                .map(|p| p.name)
                .unwrap_or("Своя настройка")
                .into();
            model.resolving = p.loading;
        }
        model
    }
    fn controlled_playback(&self) -> Option<wedex_cloud::Playback> {
        crate::platform::mobile()
            .then(|| self.cloud.view.remote_playback())
            .flatten()
    }
    fn display_status(&self) -> audio::Status {
        if let Some(p) = self.controlled_playback() {
            return audio::Status {
                track: p
                    .track
                    .as_ref()
                    .and_then(|t| crate::cloud::display_track(t).ok()),
                playing: p.playing,
                loading: p.loading,
                position: p.position as f32,
                duration: p.duration as f32,
                ..Default::default()
            };
        }
        self.preview_status
            .clone()
            .unwrap_or_else(|| self.player.status())
    }
    #[cfg(any(target_os = "android", test))]
    fn player_command(&mut self, action: wedex_cloud::Action) -> Result<(), String> {
        if self.controlled_playback().is_some() {
            self.cloud.action(crate::cloud::Action::Remote(action));
            Ok(())
        } else {
            self.remote_action(action)
        }
    }
    fn remote_action(&mut self, action: wedex_cloud::Action) -> Result<(), String> {
        use wedex_cloud::Action as Remote;
        if !action.valid() {
            return Err("Некорректная команда".into());
        }
        if crate::platform::mobile() && !self.cloud.view.own_device.is_empty() {
            self.cloud.action(crate::cloud::Action::Target(
                self.cloud.view.own_device.clone(),
            ));
        }
        let status = self
            .preview_status
            .clone()
            .unwrap_or_else(|| self.player.status());
        match action {
            Remote::SetPlaying { playing } => {
                if status.track.is_none() {
                    return Err("Сначала выберите трек".into());
                }
                if playing && status.ended {
                    if let Some(index) = self.cursor {
                        self.local_action(Intent::QueueIndex(index));
                    }
                } else if let Some(status) = &mut self.preview_status {
                    status.playing = playing;
                } else {
                    self.player.set_playing(playing);
                }
            }
            Remote::Next => self.local_action(Intent::Next),
            Remote::Previous => self.local_action(Intent::Previous),
            Remote::Seek { seconds } => {
                if status.track.is_none() || seconds > status.duration.max(0.) as f64 {
                    return Err("Позиция вне трека".into());
                }
                self.local_action(Intent::Seek(seconds as f32));
            }
            Remote::Volume { value } => self.local_action(Intent::Volume(value)),
            Remote::Repeat { enabled } => self.local_action(Intent::Repeat(enabled)),
            Remote::Shuffle { enabled } => self.local_action(Intent::Shuffle(enabled)),
            Remote::QueueIndex { index } => {
                if index >= self.queue.len() {
                    return Err("Трек отсутствует в очереди".into());
                }
                self.local_action(Intent::QueueIndex(index));
            }
            Remote::Like { key, liked } => {
                let t = self
                    .queue
                    .iter()
                    .chain(self.favorites.iter())
                    .chain(status.track.iter())
                    .find(|t| t.key() == key)
                    .cloned()
                    .ok_or("Трек недоступен")?;
                if self.favorites.iter().any(|t| t.key() == key) != liked {
                    self.local_action(Intent::Like(t));
                }
            }
            Remote::Equalizer { gains, preamp } => {
                self.local_action(Intent::Eq(gains));
                self.local_action(Intent::Preamp(preamp));
            }
            Remote::PlayTrack { track, radio } => {
                let t = crate::cloud::display_track(&track).map_err(|e| e.to_string())?;
                self.local_action(if radio {
                    Intent::Radio(t)
                } else {
                    Intent::Play(t)
                });
            }
            Remote::PlayCollection { tracks, index } => {
                let tracks = tracks
                    .iter()
                    .map(crate::cloud::display_track)
                    .collect::<anyhow::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?;
                self.local_action(Intent::PlayCollection(tracks, index));
            }
        }
        Ok(())
    }
    pub(crate) fn background_tick(&mut self) {
        if self.preview_status.is_none() {
            self.updates.tick(
                if self.cloud.view.owner_access && !self.cloud.view.server_running {
                    &self.cloud.view.server_version
                } else {
                    ""
                },
            );
        }
        #[cfg(target_os = "android")]
        if let Some(invitation) = crate::android::take_invitation() {
            self.interface.prefill_cloud_invitation(invitation);
        }
        #[cfg(target_os = "android")]
        if !crate::android::foreground() && !self.settings.close_to_tray {
            self.player.set_playing(false);
        }
        #[cfg(target_os = "android")]
        for (action, value) in crate::android::take_commands() {
            match action {
                1 => {
                    let _ = self.player_command(wedex_cloud::Action::SetPlaying { playing: true });
                }
                2 => {
                    let _ = self.player_command(wedex_cloud::Action::SetPlaying { playing: false });
                }
                3 => self.glass_action(Intent::Next),
                4 => self.glass_action(Intent::Previous),
                5 => self.glass_action(Intent::Seek(value)),
                7 => {
                    if let Some(p) = self.controlled_playback() {
                        let _ = self.player_command(wedex_cloud::Action::Volume {
                            value: (p.volume + value.signum() * 0.05).clamp(0., 1.),
                        });
                    }
                }
                8 => {
                    let _ = self.player_command(wedex_cloud::Action::Volume {
                        value: value.clamp(0., 1.),
                    });
                }
                6 => {
                    self.player.stop();
                    self.cloud.flush(self.store.as_ref());
                }
                9 => self.glass_action(Intent::Recognize),
                10 => {
                    if self.interface.overlay.is_some() {
                        self.interface.overlay = None;
                    } else if self.interface.zen {
                        self.interface.zen = false;
                        self.zen_request = Some(false);
                    } else if self.interface.page != Page::Sonora {
                        self.interface.page = Page::Sonora;
                    } else {
                        crate::android::minimize();
                    }
                }
                _ => {}
            }
        }
        if self.preview_status.is_none() {
            self.cloud
                .connection_values(std::collections::BTreeMap::from([
                    ("yandex".into(), self.yandex_token.clone()),
                    ("lastfm".into(), self.lastfm_key.clone()),
                    ("soundcloud".into(), self.soundcloud_token.clone()),
                    ("hitmo".into(), self.settings.hitmo_base.clone()),
                ]));
            self.poll();
            let deliveries = self.cloud.tick(
                crate::cloud::LocalPlayback {
                    status: &self.player.status(),
                    queue: &self.queue,
                    cursor: self.cursor,
                    settings: &self.settings,
                    shuffle: self.interface.shuffle_enabled(),
                    favorites: &self.favorites,
                },
                self.store.as_ref(),
            );
            for delivery in deliveries {
                let error = self.remote_action(delivery.command.action.clone()).err();
                self.cloud.acknowledge(&delivery, error);
            }
            for error in std::mem::take(&mut self.cloud.errors) {
                self.report_error("cloud", error);
            }
            for (name, value) in self.cloud.connection_changes() {
                let provider = match name.as_str() {
                    "yandex" => Provider::Yandex,
                    "lastfm" => Provider::LastFm,
                    "soundcloud" => Provider::SoundCloud,
                    "hitmo" => Provider::Hitmotop,
                    _ => continue,
                };
                self.save_connection_impl(provider, &value, false);
            }
        }
        if let Some(p) = self.controlled_playback() {
            if self.player.status().playing {
                self.player.set_playing(false);
            }
            self.interface.set_shuffle(p.shuffle);
        } else {
            self.interface.set_shuffle(self.queue_order.is_some());
        }
        #[cfg(target_os = "android")]
        crate::android::publish(
            &self.display_status(),
            self.controlled_playback().is_some(),
            self.controlled_playback()
                .map(|p| p.volume)
                .unwrap_or(self.settings.volume),
        );
    }
    fn glass_action(&mut self, action: Intent) {
        use wedex_cloud::Action as Remote;
        if let Some(p) = self.controlled_playback() {
            let command = match &action {
                Intent::Pause => Some(Remote::SetPlaying {
                    playing: !p.playing,
                }),
                Intent::Next => Some(Remote::Next),
                Intent::Previous => Some(Remote::Previous),
                Intent::Seek(seconds) => Some(Remote::Seek {
                    seconds: *seconds as f64,
                }),
                Intent::Volume(value) => Some(Remote::Volume { value: *value }),
                Intent::Repeat(enabled) => Some(Remote::Repeat { enabled: *enabled }),
                Intent::Shuffle(enabled) => Some(Remote::Shuffle { enabled: *enabled }),
                Intent::QueueIndex(index) => Some(Remote::QueueIndex { index: *index }),
                Intent::Like(t) => Some(Remote::Like {
                    key: t.key(),
                    liked: !p
                        .queue
                        .iter()
                        .chain(p.track.iter())
                        .any(|f| f.key == t.key() && f.liked),
                }),
                Intent::Eq(gains) => Some(Remote::Equalizer {
                    gains: *gains,
                    preamp: p.preamp,
                }),
                Intent::Preamp(preamp) => Some(Remote::Equalizer {
                    gains: p.eq,
                    preamp: *preamp,
                }),
                Intent::Preset(i) => {
                    crate::equalizer::PRESETS
                        .get(*i)
                        .map(|preset| Remote::Equalizer {
                            gains: preset.gains,
                            preamp: 0.,
                        })
                }
                Intent::Play(t) | Intent::Radio(t) => Some(Remote::PlayTrack {
                    track: Box::new(crate::cloud::track(t)),
                    radio: matches!(&action, Intent::Radio(_)),
                }),
                Intent::PlayCollection(tracks, index) => Some(Remote::PlayCollection {
                    tracks: tracks.iter().map(crate::cloud::track).collect(),
                    index: *index,
                }),
                _ => None,
            };
            if let Some(command) = command {
                self.cloud.action(crate::cloud::Action::Remote(command));
                return;
            }
        }
        self.local_action(action);
    }
    fn local_action(&mut self, action: Intent) {
        match action {
            Intent::Cloud(action) => self.cloud.action(action),
            Intent::Update(action) => {
                #[cfg(windows)]
                if matches!(action, crate::updates::Action::Server) {
                    if !self
                        .cloud
                        .view
                        .capabilities
                        .iter()
                        .any(|c| c == "server-update")
                    {
                        self.interface.overlay = Some(crate::glass::Overlay::Vps);
                    } else {
                        self.cloud.action(crate::cloud::Action::UpdateServer);
                    }
                    return;
                }
                if let Err(e) = self.updates.action(action, &self.cloud.view.server_version) {
                    self.report_error("update", e.to_string());
                }
            }
            Intent::Artist(artist) => {
                self.artist_request = self.artist_request.wrapping_add(1);
                self.artist = Some(crate::model::ArtistCatalog {
                    total: artist.tracks,
                    artist,
                    tracks: Vec::new(),
                    next_page: Some(0),
                    loading: false,
                    failed: false,
                });
                if self.preview_status.is_some() {
                    if let Some(catalog) = &mut self.artist {
                        catalog.tracks = self.results.clone();
                        catalog.next_page = None;
                        catalog.total = Some(catalog.tracks.len());
                    }
                } else {
                    self.load_artist_page();
                }
            }
            Intent::ArtistMore => {
                if self.preview_status.is_none() {
                    self.load_artist_page();
                }
            }
            Intent::Preamp(value) => {
                self.settings.eq_preamp = value.clamp(-12., 12.);
                self.settings.eq_preset = "Своя настройка".into();
                *self.player.preamp.write().unwrap() = self.settings.eq_preamp;
                self.persist();
            }
            Intent::Preset(index) => {
                if let Some(preset) = crate::equalizer::PRESETS.get(index) {
                    self.settings.eq = preset.gains;
                    self.settings.eq_preamp = 0.;
                    self.settings.eq_preset = preset.name.into();
                    *self.player.eq.write().unwrap() = preset.gains;
                    *self.player.preamp.write().unwrap() = 0.;
                    self.persist();
                }
            }
            Intent::Crossfade(seconds) => {
                self.settings.crossfade = seconds.clamp(0., 12.);
                self.persist();
            }
            Intent::CloseToTray(value) => {
                self.settings.close_to_tray = value;
                self.persist();
            }
            Intent::Lyrics(value) => {
                self.settings.lyrics_enabled = value;
                self.persist();
            }
            Intent::Repeat(value) => {
                self.settings.repeat_current = value;
                self.persist();
            }
            Intent::Zen(value) => {
                self.zen_request = Some(value);
            }
            Intent::PlayCollection(tracks, index) => self.play_collection(tracks, index),
            Intent::ResetRadio => {
                if self.preview_status.is_some() {
                    self.sonora_ready = true;
                    if self.queue.is_empty() {
                        self.queue = self.favorites.clone();
                    }
                    self.settings.last_artist.clear();
                    self.settings.last_title.clear();
                    self.playback_source = PlaybackSource::Sonora;
                    crate::discovery::Sampler::fresh().shuffle(&mut self.queue);
                    self.glass_action(Intent::QueueIndex(0));
                } else {
                    self.reset_radio();
                }
            }
            Intent::Theme(value) => {
                self.settings.theme = value;
                self.persist();
            }
            Intent::Navigate(page) => {
                self.tab = match page {
                    Page::Sonora => Tab::Sonora,
                    Page::Search | Page::Artist => Tab::Search,
                    _ => Tab::Library,
                };
            }
            Intent::Search(query) => {
                self.query = query;
                if self.preview_status.is_none() {
                    self.search();
                }
            }
            Intent::Play(track) => {
                if self.interface.page == Page::Search && self.preview_status.is_none() {
                    self.settings
                        .recent_searches
                        .retain(|t| t.key() != track.key());
                    self.settings.recent_searches.insert(0, track.clone());
                    self.settings.recent_searches.truncate(12);
                    self.persist();
                }
                if let Some(status) = &mut self.preview_status {
                    status.track = Some(track.clone());
                    status.duration = track.duration as f32;
                    status.position = 0.;
                    status.playing = true;
                    self.queue.push(track);
                    self.cursor = Some(self.queue.len() - 1);
                } else {
                    self.handle_action(Action::Play(track));
                }
            }
            Intent::Like(track) => {
                if self.preview_status.is_some() {
                    if self.favorites.iter().any(|t| t.key() == track.key()) {
                        self.favorites.retain(|t| t.key() != track.key());
                    } else {
                        self.favorites.push(track);
                    }
                } else {
                    self.handle_action(Action::Like(track));
                }
            }
            Intent::Radio(track) => {
                self.start_after_library = false;
                self.sonora_ready = true;
                if self.interface.page == Page::Search && self.preview_status.is_none() {
                    self.settings
                        .recent_searches
                        .retain(|t| t.key() != track.key());
                    self.settings.recent_searches.insert(0, track.clone());
                    self.settings.recent_searches.truncate(12);
                }
                if self.preview_status.is_some() {
                    self.settings.last_artist = track.artist.clone();
                    self.settings.last_title = track.title.clone();
                    self.glass_action(Intent::Play(track));
                    self.playback_source = PlaybackSource::Sonora;
                } else {
                    self.handle_action(Action::Radio(track));
                }
            }
            Intent::QueueIndex(index) => {
                if let Some(status) = &mut self.preview_status {
                    if let Some(track) = self.queue.get(index) {
                        status.track = Some(track.clone());
                        status.position = 0.;
                        status.duration = track.duration as f32;
                        status.playing = true;
                        status.ended = false;
                        self.cursor = Some(index);
                    }
                } else {
                    self.start(index);
                }
            }
            Intent::Pause => {
                if let Some(status) = &mut self.preview_status {
                    status.playing = !status.playing;
                } else {
                    self.player.pause();
                }
            }
            Intent::Next => {
                if self.preview_status.is_some() {
                    if let Some(i) = self.cursor {
                        self.glass_action(Intent::QueueIndex((i + 1) % self.queue.len().max(1)));
                    }
                } else {
                    self.next();
                }
            }
            Intent::Previous => {
                let status = self
                    .preview_status
                    .clone()
                    .unwrap_or_else(|| self.player.status());
                if status.position > 3. {
                    self.glass_action(Intent::Seek(0.));
                } else if let Some(index) = self.cursor {
                    self.glass_action(Intent::QueueIndex(index.saturating_sub(1)));
                }
            }
            Intent::Seek(position) => {
                self.cloud.discontinuity();
                if let Some(status) = &mut self.preview_status {
                    status.position = position;
                } else {
                    self.player.seek(position);
                }
            }
            Intent::Volume(value) => {
                self.settings.volume = value;
                self.player.volume(value);
                self.persist();
            }
            Intent::Eq(gains) => {
                self.settings.eq_preset = "Своя настройка".into();
                self.settings.eq = gains;
                *self.player.eq.write().unwrap() = gains;
                self.persist();
            }
            Intent::Discovery(value) => {
                self.settings.discovery = value;
                self.persist();
                if self.preview_status.is_none() {
                    self.launch_radio();
                }
            }
            Intent::Sync => {
                if self.preview_status.is_none() {
                    self.sync();
                }
            }
            Intent::Playlist(index) => {
                if let Some(playlist) = self.playlists.get(index).cloned() {
                    self.playlist_title = playlist.title.clone();
                    if self.preview_status.is_none() {
                        self.playlist_tracks.clear();
                        self.loading_playlist = true;
                        self.playlist_request += 1;
                        let id = self.playlist_request;
                        self.job(move |s| {
                            Event::Playlist(
                                id,
                                s.yandex_playlist(&playlist).map_err(|e| e.to_string()),
                            )
                        });
                    }
                }
            }
            Intent::Versions(track) => {
                if self.preview_status.is_some() {
                    self.alternatives = Some((
                        track,
                        self.favorites.iter().take(3).cloned().collect(),
                        vec![],
                    ));
                } else {
                    self.handle_action(Action::Versions(track));
                }
            }
            Intent::SaveVersion(original, mut replacement) => {
                replacement.edition = Edition::ConfirmedOriginal;
                if let Some(store) = &self.store {
                    match store.replace(&original, &replacement) {
                        Ok(()) => self.message = "Оригинальная версия сохранена".into(),
                        Err(e) => self.report_error("operation", e.to_string()),
                    }
                }
                self.alternatives = None;
            }
            Intent::ClearVersion(original) => {
                if let Some(store) = &self.store {
                    match store.clear_replacement(&original) {
                        Ok(()) => self.message = "Сохранённая замена сброшена".into(),
                        Err(e) => self.report_error("operation", e.to_string()),
                    }
                }
                self.alternatives = None;
            }
            Intent::Open(url) => self.handle_action(Action::Open(url)),
            Intent::Shuffle(enabled) => {
                if self.interface.shuffle_enabled() == enabled
                    && self.queue_order.is_some() == enabled
                {
                    return;
                }
                self.interface.set_shuffle(enabled);
                let start = self
                    .cursor
                    .map(|i| i + 1)
                    .unwrap_or(0)
                    .min(self.queue.len());
                if enabled {
                    use std::hash::{BuildHasher, Hasher};
                    self.queue_order = Some(self.queue.iter().map(Track::key).collect());
                    let state = std::collections::hash_map::RandomState::new();
                    self.queue[start..].sort_by_cached_key(|t| {
                        let mut h = state.build_hasher();
                        h.write(t.key().as_bytes());
                        h.finish()
                    });
                } else if let Some(order) = self.queue_order.take() {
                    self.queue[start..].sort_by_cached_key(|t| {
                        order
                            .iter()
                            .position(|key| *key == t.key())
                            .unwrap_or(usize::MAX)
                    });
                }
            }
            Intent::Recognize => {
                #[cfg(target_os = "android")]
                if !crate::android::request_microphone() {
                    return;
                }
                if self.preview_status.is_none() {
                    self.record();
                }
            }
            Intent::CancelRecognition => {
                if self.recording.is_some() {
                    self.record();
                }
            }
            Intent::CheckConnection(provider, value, revision) => {
                if self.preview_status.is_some() {
                    return;
                }
                self.job(move |services| {
                    let result = services
                        .check_connection(provider, &value)
                        .map_err(|e| e.to_string());
                    Event::ConnectionChecked(provider, revision, value, result)
                });
            }
            Intent::SaveConnection(provider, value) => {
                self.save_connection(provider, &value);
            }
            Intent::PreferOriginal(value) => {
                self.settings.prefer_original = value;
                self.persist();
            }
        }
    }
}

fn should_crossfade(status: &audio::Status, settings: &Settings, handled: Option<u64>) -> bool {
    settings.crossfade > 0.
        && !settings.repeat_current
        && status.playing
        && !status.loading
        && !status.ended
        && status.duration > settings.crossfade + 1.
        && status.duration - status.position <= settings.crossfade
        && handled != Some(status.ticket)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if cfg!(windows)
            && (self.preview_status.is_none() || std::env::var_os("WEDEX_PREVIEW_TRAY").is_some())
            && !self.tray_attempted
        {
            self.tray_attempted = true;
            match crate::tray::Tray::new(ctx) {
                Ok(tray) => self.tray = Some(tray),
                Err(e) => self.report_error("tray", e.to_string()),
            }
        }
        for action in self.tray.as_ref().map(|t| t.poll()).unwrap_or_default() {
            match action {
                crate::tray::Action::Show => {
                    self.hidden = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                crate::tray::Action::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested())
            && self.settings.close_to_tray
            && self.tray.is_some()
            && !self.quitting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hidden = true;
            self.persist();
        }
        if let Some(zen) = self.zen_request.take() {
            #[cfg(target_os = "android")]
            crate::android::fullscreen(zen);
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            if zen {
                self.zen_previous_fullscreen.get_or_insert(fullscreen);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            } else if let Some(previous) = self.zen_previous_fullscreen.take() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(previous));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
        }
        self.background_tick();
        self.interface.cloud_view = self.cloud.view.clone();
        if self.updates.exit {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.updates.prompt && self.interface.overlay.is_none() {
            self.interface.overlay = Some(crate::glass::Overlay::Updates);
            self.updates.prompt = false;
        }
        if self.hidden {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        ctx.set_theme(match self.settings.theme {
            ThemePreference::System => {
                #[cfg(target_os = "android")]
                {
                    if crate::android::system_dark() {
                        egui::ThemePreference::Dark
                    } else {
                        egui::ThemePreference::Light
                    }
                }
                #[cfg(not(target_os = "android"))]
                {
                    egui::ThemePreference::System
                }
            }
            ThemePreference::Light => egui::ThemePreference::Light,
            ThemePreference::Dark => egui::ThemePreference::Dark,
        });
        let model = self.glass_model();
        let actions = self.interface.show(ctx, &model);
        for action in actions {
            self.glass_action(action);
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = _gl {
            self.interface.destroy(gl);
        }
        self.persist();
        self.cloud.flush(self.store.as_ref());
        if let Some((_, cancel)) = &self.recording {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phone_main_player_controls_pc_over_real_tls_without_changing_local_audio() {
        let _mobile = crate::platform::mobile_for_test();
        let directory =
            std::env::temp_dir().join(format!("wedex-player-ui-{}", uuid::Uuid::new_v4()));
        let server = wedex_cloud::server::start(wedex_cloud::server::Config {
            data_dir: directory.clone(),
            bind: "127.0.0.1:0".parse().unwrap(),
        })
        .unwrap();
        let pc_connection = wedex_cloud::client::blocking_pair(
            &server.invitation(false).unwrap(),
            "PC fixture",
            true,
        )
        .unwrap();
        let phone_connection = wedex_cloud::client::blocking_pair(
            &server.invitation(false).unwrap(),
            "Phone fixture",
            true,
        )
        .unwrap();
        let pc_id = pc_connection.credentials.device_id.clone();
        let ctx = egui::Context::default();
        let mut pc = App::with_context(&ctx, Some("sonora".into()));
        pc.queue[0].cover = Some("https://example.test/pc-artwork.jpg".into());
        pc.preview_status.as_mut().unwrap().track = Some(pc.queue[0].clone());
        pc.cloud = crate::cloud::Cloud::connected_for_test(pc_connection);
        pc.cloud.action(crate::cloud::Action::Target(pc_id.clone()));
        let mut phone = App::with_context(&ctx, Some("sonora".into()));
        phone.preview_status = Some(audio::Status::default());
        phone.cloud = crate::cloud::Cloud::connected_for_test(phone_connection);
        let local_volume = phone.settings.volume;
        let local_favorites = phone.favorites.len();
        fn pump(pc: &mut App, phone: &mut App, done: impl Fn(&App, &App) -> bool) {
            let until = Instant::now() + std::time::Duration::from_secs(8);
            while Instant::now() < until {
                for app in [&mut *pc, &mut *phone] {
                    let status = app.preview_status.clone().unwrap();
                    let deliveries = app.cloud.tick(
                        crate::cloud::LocalPlayback {
                            status: &status,
                            queue: &app.queue,
                            cursor: app.cursor,
                            settings: &app.settings,
                            shuffle: app.interface.shuffle_enabled(),
                            favorites: &app.favorites,
                        },
                        None,
                    );
                    for d in deliveries {
                        let error = app.remote_action(d.command.action.clone()).err();
                        app.cloud.acknowledge(&d, error);
                    }
                }
                if done(pc, phone) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("Cloud player state did not reach expected result");
        }
        pump(&mut pc, &mut phone, |_, p| {
            p.controlled_playback().is_some_and(|s| s.track.is_some())
        });
        assert_eq!(phone.cloud.view.selected, pc_id);
        let model = phone.glass_model();
        assert_eq!(
            model.status.track.unwrap().cover.as_deref(),
            Some("https://example.test/pc-artwork.jpg")
        );
        assert_eq!(model.queue.len(), pc.queue.len());
        phone.glass_action(Intent::Pause);
        pump(&mut pc, &mut phone, |c, p| {
            !c.preview_status.as_ref().unwrap().playing && !p.display_status().playing
        });
        phone
            .player_command(wedex_cloud::Action::Volume { value: 0.23 })
            .unwrap();
        pump(&mut pc, &mut phone, |c, p| {
            (c.settings.volume - 0.23).abs() < 0.001
                && (p.glass_model().settings.volume - 0.23).abs() < 0.001
        });
        phone.glass_action(Intent::Seek(81.));
        pump(&mut pc, &mut phone, |c, p| {
            c.preview_status.as_ref().unwrap().position == 81. && p.display_status().position >= 81.
        });
        let current = phone.display_status().track.unwrap();
        let liked = phone.glass_model().player_liked.unwrap();
        phone.glass_action(Intent::Like(current.clone()));
        pump(&mut pc, &mut phone, |c, p| {
            c.favorites.iter().any(|t| t.key() == current.key()) != liked
                && p.glass_model().player_liked == Some(!liked)
        });
        phone.glass_action(Intent::Eq([3.; 10]));
        pump(&mut pc, &mut phone, |c, p| {
            c.settings.eq == [3.; 10] && p.glass_model().settings.eq == [3.; 10]
        });
        assert_eq!(phone.settings.volume, local_volume);
        assert_eq!(phone.favorites.len(), local_favorites);
        assert!(!phone.preview_status.as_ref().unwrap().playing);
        phone.glass_action(Intent::Next);
        pump(&mut pc, &mut phone, |c, p| {
            c.cursor == Some(1) && p.glass_model().cursor == Some(1)
        });
        phone.cloud.action(crate::cloud::Action::Target(
            phone.cloud.view.own_device.clone(),
        ));
        assert!(phone.controlled_playback().is_none());
        drop(phone);
        drop(pc);
        drop(server);
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn remote_commands_control_the_actual_dispatcher_and_absolute_pause_is_idempotent() {
        use wedex_cloud::Action as Remote;
        let ctx = egui::Context::default();
        let mut app = App::with_context(&ctx, Some("sonora".into()));
        assert!(app.preview_status.as_ref().unwrap().playing);
        app.remote_action(Remote::SetPlaying { playing: false })
            .unwrap();
        app.remote_action(Remote::SetPlaying { playing: false })
            .unwrap();
        assert!(!app.preview_status.as_ref().unwrap().playing);
        app.remote_action(Remote::Volume { value: 0.23 }).unwrap();
        assert_eq!(app.settings.volume, 0.23);
        app.remote_action(Remote::Seek { seconds: 72. }).unwrap();
        assert_eq!(app.preview_status.as_ref().unwrap().position, 72.);
        app.remote_action(Remote::QueueIndex { index: 2 }).unwrap();
        assert_eq!(app.cursor, Some(2));
        assert_eq!(
            app.preview_status
                .as_ref()
                .unwrap()
                .track
                .as_ref()
                .unwrap()
                .key(),
            app.queue[2].key()
        );
        assert!(app.remote_action(Remote::Seek { seconds: 9999. }).is_err());
        assert!(app
            .remote_action(Remote::QueueIndex { index: 9999 })
            .is_err());
        app.remote_action(Remote::Shuffle { enabled: true })
            .unwrap();
        let order = app.queue.iter().map(Track::key).collect::<Vec<_>>();
        app.remote_action(Remote::Shuffle { enabled: true })
            .unwrap();
        assert_eq!(order, app.queue.iter().map(Track::key).collect::<Vec<_>>());
    }
    #[test]
    #[cfg(windows)]
    #[ignore = "Runs an isolated native window to verify tray actions while the window is hidden"]
    fn native_tray_wakes_the_hidden_window_and_quit_exits_instead_of_hiding_again() {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        struct Check {
            app: App,
            started: bool,
            restored: Arc<AtomicBool>,
            quit: Arc<AtomicBool>,
        }
        impl eframe::App for Check {
            fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
                self.app.update(ctx, frame);
                if !self.started {
                    self.started = true;
                    self.app.hidden = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    let tray = self.app.tray.as_ref().unwrap();
                    tray.schedule_test_action(
                        crate::tray::Action::Show,
                        ctx,
                        std::time::Duration::from_millis(400),
                    );
                    tray.schedule_test_action(
                        crate::tray::Action::Quit,
                        ctx,
                        std::time::Duration::from_millis(1200),
                    );
                    ctx.request_repaint();
                } else if !self.app.hidden {
                    self.restored.store(true, Ordering::Relaxed);
                }
                if self.app.quitting {
                    self.quit.store(true, Ordering::Relaxed);
                }
            }
            fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
                self.app.on_exit(gl);
            }
        }
        let restored = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        let observed = restored.clone();
        let exited = quit.clone();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([480., 600.]),
            event_loop_builder: Some(Box::new(|b| {
                b.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "WeDex tray verification",
            options,
            Box::new(move |cc| {
                let mut app = App::with_context(&cc.egui_ctx, Some("sonora".into()));
                app.tray = Some(crate::tray::Tray::new(&cc.egui_ctx).unwrap());
                app.tray_attempted = true;
                Ok(Box::new(Check {
                    app,
                    started: false,
                    restored: observed,
                    quit: exited,
                }))
            }),
        )
        .unwrap();
        assert!(
            restored.load(Ordering::Relaxed),
            "Show action must wake and restore the hidden window"
        );
        assert!(
            quit.load(Ordering::Relaxed),
            "Quit action must bypass close-to-tray"
        );
    }
    fn app() -> App {
        let mut app = App::with_context(&egui::Context::default(), Some("favorites".into()));
        app.preview_status = None;
        app
    }
    #[test]
    fn changing_hitmo_does_not_modify_other_connections_and_old_library_responses_are_ignored() {
        let mut app = app();
        app.services.yandex = "unchanged-yandex".into();
        app.services.lastfm = "unchanged-lastfm".into();
        app.services.soundcloud = "unchanged-soundcloud".into();
        app.save_connection(Provider::Hitmotop, "mirror.example");
        assert_eq!(app.settings.hitmo_base, "https://mirror.example/");
        assert_eq!(app.services.hitmo_base, "https://mirror.example/");
        assert_eq!(app.services.yandex, "unchanged-yandex");
        assert_eq!(app.services.lastfm, "unchanged-lastfm");
        assert_eq!(app.services.soundcloud, "unchanged-soundcloud");
        let favorites = app.favorites.iter().map(Track::key).collect::<Vec<_>>();
        app.queue.clear();
        app.cursor = None;
        app.library_request = 10;
        app.loading_library = true;
        app.tx
            .send(Event::Library(
                9,
                Ok((Vec::new(), Vec::new(), "Old account".into())),
            ))
            .unwrap();
        app.poll();
        assert_ne!(app.account, "Old account");
        assert!(app.loading_library);
        assert_eq!(
            app.favorites.iter().map(Track::key).collect::<Vec<_>>(),
            favorites
        );
    }
    #[test]
    fn search_merges_partial_results_and_ignores_batches_from_previous_queries() {
        let mut app = app();
        app.search_id = 12;
        app.searching = true;
        let yandex = crate::model::tests::track("Song", "Artist", 100);
        let mut hitmo = yandex.clone();
        hitmo.provider = Provider::Hitmotop;
        app.tx
            .send(Event::Search(
                11,
                SearchResult {
                    tracks: vec![yandex.clone()],
                    ..Default::default()
                },
                true,
            ))
            .unwrap();
        app.tx
            .send(Event::Search(
                12,
                SearchResult {
                    tracks: vec![hitmo],
                    ..Default::default()
                },
                false,
            ))
            .unwrap();
        app.poll();
        assert!(app.searching);
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].provider, Provider::Hitmotop);
        app.tx
            .send(Event::Search(
                12,
                SearchResult {
                    tracks: vec![yandex.clone(), yandex],
                    ..Default::default()
                },
                false,
            ))
            .unwrap();
        app.poll();
        assert!(app.searching);
        assert_eq!(app.results.len(), 2);
        assert_eq!(app.results[0].provider, Provider::Yandex);
        app.tx
            .send(Event::Search(12, SearchResult::default(), true))
            .unwrap();
        app.poll();
        assert!(!app.searching);
        assert_eq!(app.results.len(), 2);
    }
    #[test]
    fn fresh_sonora_waits_for_click_and_library_sync_before_using_favorites() {
        let mut directed =
            App::with_context(&egui::Context::default(), Some("sonora-start".into()));
        directed.start_after_library = true;
        directed.play_collection(directed.favorites.clone(), 0);
        assert!(!directed.start_after_library && !directed.sonora_ready);
        directed.glass_action(Intent::Radio(directed.favorites[0].clone()));
        assert!(directed.sonora_ready);
        let mut app = App::with_context(&egui::Context::default(), Some("sonora-start".into()));
        assert!(!app.sonora_ready && !app.generating && !app.start_after_library);
        assert!(app.queue.is_empty());
        app.preview_status = None;
        app.favorites.clear();
        app.loading_library = true;
        app.settings.last_artist = "Previous artist".into();
        app.settings.last_title = "Previous seed".into();
        app.reset_radio();
        assert!(app.start_after_library);
        assert!(app.settings.last_artist.is_empty() && app.settings.last_title.is_empty());
        app.tx
            .send(Event::Library(0, Err("fixture offline".into())))
            .unwrap();
        app.poll();
        assert!(!app.start_after_library && !app.sonora_ready);
        app.loading_library = true;
        app.reset_radio();
        let track = crate::model::tests::track("Favorite seed", "Artist", 100);
        app.tx
            .send(Event::Library(
                0,
                Ok((vec![track.clone()], vec![], "Fixture".into())),
            ))
            .unwrap();
        app.poll();
        assert!(!app.start_after_library && !app.sonora_ready);
        assert_eq!(app.settings.recent_radio_seeds, vec![track.identity()]);
        app.tx
            .send(Event::Recommendations(app.radio_id, Ok(vec![track]), false))
            .unwrap();
        app.poll();
        assert!(app.sonora_ready);
        assert_eq!(app.queue.len(), 1);
    }
    #[test]
    fn artist_catalog_ignores_stale_requests_and_keeps_pagination_retryable() {
        let mut app = app();
        app.artist_request = 9;
        let artist = crate::model::Artist {
            id: "42".into(),
            name: "Artist".into(),
            cover: None,
            tracks: Some(3),
            listeners: None,
        };
        app.artist = Some(crate::model::ArtistCatalog {
            artist,
            tracks: Vec::new(),
            total: Some(3),
            next_page: Some(0),
            loading: true,
            failed: false,
        });
        let first = app.favorites[0].clone();
        let second = app.favorites[1].clone();
        app.tx
            .send(Event::Artist(
                8,
                0,
                Ok(crate::model::ArtistPage {
                    tracks: vec![second.clone()],
                    ..Default::default()
                }),
            ))
            .unwrap();
        app.poll();
        assert!(app.artist.as_ref().unwrap().tracks.is_empty());
        assert!(app.artist.as_ref().unwrap().loading);
        app.tx
            .send(Event::Artist(
                9,
                0,
                Ok(crate::model::ArtistPage {
                    tracks: vec![first.clone()],
                    total: Some(3),
                    next_page: Some(1),
                    ..Default::default()
                }),
            ))
            .unwrap();
        app.poll();
        assert_eq!(app.artist.as_ref().unwrap().tracks.len(), 1);
        app.tx
            .send(Event::Artist(9, 1, Err("fixture offline".into())))
            .unwrap();
        app.poll();
        let catalog = app.artist.as_ref().unwrap();
        assert!(catalog.failed);
        assert_eq!(catalog.next_page, Some(1));
        assert_eq!(catalog.tracks.len(), 1);
        app.tx
            .send(Event::Artist(
                9,
                1,
                Ok(crate::model::ArtistPage {
                    tracks: vec![first, second],
                    next_page: None,
                    ..Default::default()
                }),
            ))
            .unwrap();
        app.poll();
        assert_eq!(app.artist.as_ref().unwrap().tracks.len(), 2);
        assert_eq!(app.artist.as_ref().unwrap().next_page, None);
    }
    #[test]
    fn crossfade_only_starts_once_near_end_with_repeating_disabled() {
        let mut settings = Settings {
            crossfade: 4.,
            ..Default::default()
        };
        let mut status = audio::Status {
            ticket: 7,
            playing: true,
            duration: 200.,
            position: 195.,
            ..Default::default()
        };
        assert!(!should_crossfade(&status, &settings, None));
        status.position = 196.1;
        assert!(should_crossfade(&status, &settings, None));
        assert!(!should_crossfade(&status, &settings, Some(7)));
        settings.repeat_current = true;
        assert!(!should_crossfade(&status, &settings, None));
        settings.repeat_current = false;
        status.loading = true;
        assert!(!should_crossfade(&status, &settings, None));
    }
    #[test]
    fn lyrics_are_opt_in_cached_and_stale_results_do_not_replace_the_current_song() {
        let mut app = app();
        let mut status = audio::Status {
            track: Some(app.queue[0].clone()),
            ..Default::default()
        };
        app.prepare_lyrics(&status);
        assert!(app.lyrics_key.is_none());
        let first = app.queue[0].key();
        let second = app.queue[1].key();
        app.lyrics_cache.insert(
            first.clone(),
            Some(crate::lyrics::Lyrics {
                plain: "First fixture".into(),
                ..Default::default()
            }),
        );
        app.lyrics_cache.insert(
            second.clone(),
            Some(crate::lyrics::Lyrics {
                plain: "Second fixture".into(),
                ..Default::default()
            }),
        );
        app.glass_action(Intent::Lyrics(true));
        app.prepare_lyrics(&status);
        assert_eq!(app.lyrics.as_ref().unwrap().plain, "First fixture");
        status.track = Some(app.queue[1].clone());
        app.prepare_lyrics(&status);
        app.tx.send(Event::Lyrics(first, Ok(None))).unwrap();
        app.poll();
        assert_eq!(app.lyrics.as_ref().unwrap().plain, "Second fixture");
        assert!(app.settings.lyrics_enabled);
        app.glass_action(Intent::Lyrics(false));
        assert!(!app.settings.lyrics_enabled);
    }
    #[test]
    fn repeat_keeps_the_current_collection_song_and_equalizer_changes_reach_audio() {
        let mut app = app();
        app.play_collection(app.favorites.clone(), 2);
        app.glass_action(Intent::Repeat(true));
        app.player.finish_for_test();
        app.poll();
        assert_eq!(app.cursor, Some(2));
        assert!(app.player.status().playing);
        app.glass_action(Intent::Preset(4));
        assert_eq!(
            *app.player.eq.read().unwrap(),
            crate::equalizer::PRESETS[4].gains
        );
        app.glass_action(Intent::Preamp(-9.));
        assert_eq!(*app.player.preamp.read().unwrap(), -9.);
        assert_eq!(app.settings.eq_preset, "Своя настройка");
    }
    #[test]
    fn favorites_advance_in_order_and_stop_at_end_even_with_sonora_open() {
        let mut app = app();
        let tracks = app.favorites.clone();
        let keys = tracks.iter().map(Track::key).collect::<Vec<_>>();
        app.glass_action(Intent::PlayCollection(tracks, 1));
        app.glass_action(Intent::Navigate(Page::Sonora));
        let request = app.radio_id;
        for (index, key) in keys.iter().enumerate().skip(2) {
            app.player.finish_for_test();
            app.poll();
            assert_eq!(app.cursor, Some(index));
            assert_eq!(&app.player.status().track.unwrap().key(), key);
            assert_eq!(app.playback_source, PlaybackSource::Collection);
            assert!(!app.generating);
        }
        app.player.finish_for_test();
        app.poll();
        assert_eq!(app.cursor, Some(keys.len() - 1));
        assert!(!app.player.status().playing);
        assert!(app.player.status().ended);
        assert_eq!(app.radio_id, request);
        assert_eq!(app.queue.iter().map(Track::key).collect::<Vec<_>>(), keys);
    }
    #[test]
    fn late_recommendations_cannot_replace_or_extend_a_collection() {
        let mut app = app();
        app.play_collection(app.playlist_tracks.clone(), 0);
        let keys = app.queue.iter().map(Track::key).collect::<Vec<_>>();
        app.tx
            .send(Event::Recommendations(
                app.radio_id,
                Ok(crate::glass::demo_tracks()),
                true,
            ))
            .unwrap();
        app.poll();
        assert_eq!(app.queue.iter().map(Track::key).collect::<Vec<_>>(), keys);
        assert_eq!(app.cursor, Some(0));
        assert_eq!(app.playback_source, PlaybackSource::Collection);
    }
    #[test]
    fn reset_cancels_the_old_request_and_uses_favorites_instead_of_manual_seed() {
        let mut app = app();
        app.settings.last_artist = "Old manual artist".into();
        app.settings.last_title = "Old seed".into();
        app.generating = true;
        app.radio_id = 42;
        app.past.insert("old|heard".into());
        let favorites = app
            .favorites
            .iter()
            .map(Track::identity)
            .collect::<HashSet<_>>();
        app.reset_radio();
        assert!(app.settings.last_artist.is_empty() && app.settings.last_title.is_empty());
        assert!(app.radio_id > 42);
        assert_eq!(app.playback_source, PlaybackSource::Sonora);
        assert!(app.queue.is_empty());
        assert!(app.past.is_empty());
        assert!(app.generating);
        assert_eq!(app.settings.recent_radio_seeds.len(), 4);
        assert!(app
            .settings
            .recent_radio_seeds
            .iter()
            .all(|s| favorites.contains(s)));
        app.tx
            .send(Event::Recommendations(
                42,
                Ok(crate::glass::demo_tracks()),
                true,
            ))
            .unwrap();
        app.poll();
        assert!(app.queue.is_empty());
    }
}
