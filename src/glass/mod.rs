//! Native, data-bound rendering of the approved Figma document.
//! The bundled scene contains measured layers, never a rasterized screen.
mod assets;
mod blur;
mod cloud;
mod composite;
mod connections;
mod layout;
mod lyrics_view;
mod mobile_player;
mod motion;
mod panels;
mod render;
mod scene;
mod search;
mod sonora;
mod theme;
mod transitions;
mod updates;
#[cfg(windows)]
mod vps;

use crate::{
    audio::Status,
    model::{self, Edition, Provider, Track},
    providers::yandex::Playlist,
    store::{Settings, ThemePreference},
};
use eframe::egui::{self, pos2, vec2, Color32, FontFamily, FontId, Rect};
use scene::{Node, Transform};
use std::collections::HashMap;

struct ListViewport {
    rect: Rect,
    key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Sonora,
    Search,
    Artist,
    Library,
    Favorites,
    Playlists,
    Playlist,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Updates,
    #[cfg(windows)]
    Vps,
    Devices,
    Cloud,
    Artists,
    Settings,
    Queue,
    Eq,
    Tune,
    Sources,
    Credentials,
    Recognition,
    Recognized,
    Versions,
}
#[derive(Clone)]
pub enum Intent {
    Update(crate::updates::Action),
    Cloud(crate::cloud::Action),
    Artist(model::Artist),
    ArtistMore,
    Theme(ThemePreference),
    Navigate(Page),
    Search(String),
    Play(Track),
    PlayCollection(Vec<Track>, usize),
    QueueIndex(usize),
    Like(Track),
    Radio(Track),
    ResetRadio,
    Pause,
    Next,
    Previous,
    Seek(f32),
    Volume(f32),
    Eq([f32; 10]),
    Preamp(f32),
    Preset(usize),
    Crossfade(f32),
    CloseToTray(bool),
    Lyrics(bool),
    Zen(bool),
    Repeat(bool),
    Discovery(f32),
    Sync,
    Playlist(usize),
    Versions(Track),
    SaveVersion(Track, Track),
    ClearVersion(Track),
    Open(String),
    Shuffle(bool),
    Recognize,
    CancelRecognition,
    CheckConnection(Provider, String, u64),
    SaveConnection(Provider, String),
    PreferOriginal(bool),
}
#[derive(Clone)]
pub(super) enum Control {
    Update(crate::updates::Action),
    #[cfg(windows)]
    VpsStart(bool),
    #[cfg(windows)]
    VpsAuth(bool),
    Cloud(crate::cloud::Action),
    CloudConnect,
    CloudCopy(bool),
    CloudLan,
    Artist(model::Artist),
    ArtistMore,
    ArtistSources(String),
    ArtistTrack(usize),
    Noop,
    ResetRadio,
    Theme(ThemePreference),
    Navigate(Page),
    Overlay(Overlay),
    Close,
    PlayPause,
    Next,
    Previous,
    Shuffle,
    Mute,
    Seek,
    Volume,
    EqBand(usize),
    Like(Track),
    Play(Track),
    Radio(Track),
    Row(Track),
    QueueIndex(usize),
    Playlist(usize),
    Search,
    Mic,
    Sync,
    Listen(bool),
    Preset(usize),
    Preamp,
    Crossfade,
    CrossfadeOff,
    CloseToTray(bool),
    ToggleLyrics,
    LyricSeek(f32),
    VerticalVolume,
    Zen(bool),
    Repeat,
    Discovery(bool),
    ApplyTune,
    Source(String),
    SaveConnection,
    CheckConnection,
    PreferOriginal,
    VersionSelect(usize),
    VersionPreview(usize),
    VersionApply,
    RecognizedSearch,
    Open(String),
}

pub struct Model {
    pub updates: crate::updates::View,
    pub player_liked: Option<bool>,
    pub sonora_ready: bool,
    pub sonora_loading: bool,
    pub artists: Vec<model::Artist>,
    pub artist: Option<model::ArtistCatalog>,
    pub error_generation: u64,
    pub settings: Settings,
    pub status: Status,
    pub queue: Vec<Track>,
    pub cursor: Option<usize>,
    pub favorites: Vec<Track>,
    pub results: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub playlist_tracks: Vec<Track>,
    pub playlist_title: String,
    pub query: String,
    pub searching: bool,
    pub loading_library: bool,
    pub loading_playlist: bool,
    pub resolving: bool,
    pub recording: bool,
    pub account: String,
    pub connected: [bool; 5],
    pub lyrics: Option<crate::lyrics::Lyrics>,
    pub lyrics_loading: bool,
    pub alternatives: Option<(Track, Vec<Track>, Vec<String>)>,
    pub versions_loading: bool,
    pub message: String,
    pub preview: bool,
}

pub struct GlassUi {
    touch_start: Option<(egui::Pos2, f64)>,
    touch_delta: f32,
    touch_long: bool,
    pub cloud_view: crate::cloud::View,
    cloud_editor: cloud::Editor,
    #[cfg(windows)]
    vps_editor: vps::Editor,
    lyrics_manual_until: f64,
    lyrics_manual_index: Option<usize>,
    lyrics_wheel_remainder: f32,
    lyrics_manual_visibility: transitions::Tween,
    lyrics_manual_layout: Option<(bool, egui::Vec2)>,
    buttons: transitions::Buttons,
    lyrics_motion: transitions::Tween,
    lyrics_emphasis: HashMap<usize, transitions::Tween>,
    zen_controls: transitions::Tween,
    lyrics_visibility: transitions::Tween,
    last_lyrics: Option<Node>,
    volume_visibility: transitions::Tween,
    last_overlay: Option<Node>,
    closing_overlay: Option<Node>,
    overlay_closed_at: f64,
    pub zen: bool,
    volume_until: f64,
    lyrics_scroll_max: f32,
    lyrics_track: Option<String>,
    lyrics_fitted_font: Option<(egui::Vec2, f32)>,
    dark: bool,
    layout: layout::Layout,
    page_scroll_max: f32,
    dialog_scroll_max: f32,
    last_error_generation: u64,
    seek_preview: Option<f32>,
    roots: HashMap<String, Node>,
    renderer: render::Renderer,
    pub page: Page,
    pub overlay: Option<Overlay>,
    pub recognized: Option<Track>,
    query: String,
    submitted_query: String,
    external_query: String,
    search_filter: usize,
    library_selection: usize,
    selected_version: Option<usize>,
    discovery: bool,
    shuffle: bool,
    muted_volume: f32,
    offsets: HashMap<String, f32>,
    drag: Option<(Control, Transform, egui::Vec2)>,
    carousel: motion::Carousel,
    last_root: Option<Node>,
    page_from: Option<Node>,
    rendered_page: Page,
    page_started: f64,
    rendered_overlay: Option<Overlay>,
    overlay_started: f64,
    last_message: String,
    message_at: f64,
    connection: connections::Editor,
    pub yandex: String,
    pub lastfm: String,
    pub soundcloud: String,
    pub reduced_motion: bool,
}
impl GlassUi {
    pub fn shuffle_enabled(&self) -> bool {
        self.shuffle
    }
    pub fn set_shuffle(&mut self, enabled: bool) {
        self.shuffle = enabled;
    }
    pub fn destroy(&self, gl: &eframe::glow::Context) {
        self.renderer.destroy(gl);
    }
    #[cfg(target_os = "android")]
    pub fn prefill_cloud_invitation(&mut self, invitation: String) {
        self.cloud_editor.invitation = invitation;
        self.overlay = Some(Overlay::Cloud);
    }
    pub fn new() -> Self {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/ui/scene.json"))
                .expect("Bundled Figma scene");
        let nodes: Vec<Node> =
            serde_json::from_value(data["roots"].clone()).expect("Figma scene schema");
        Self {
            touch_start: None,
            touch_delta: 0.,
            touch_long: false,
            cloud_view: Default::default(),
            cloud_editor: Default::default(),
            #[cfg(windows)]
            vps_editor: Default::default(),
            lyrics_manual_until: -1.,
            lyrics_manual_index: None,
            lyrics_wheel_remainder: 0.,
            lyrics_manual_visibility: transitions::Tween::new(0.),
            lyrics_manual_layout: None,
            buttons: Default::default(),
            lyrics_motion: Default::default(),
            lyrics_emphasis: HashMap::new(),
            zen_controls: transitions::Tween::new(0.),
            lyrics_visibility: transitions::Tween::new(0.),
            last_lyrics: None,
            volume_visibility: transitions::Tween::new(0.),
            last_overlay: None,
            closing_overlay: None,
            overlay_closed_at: 0.,
            zen: false,
            volume_until: -1.,
            lyrics_scroll_max: 0.,
            lyrics_track: None,
            lyrics_fitted_font: None,
            dark: false,
            layout: layout::Layout::new(vec2(1440., 960.)),
            page_scroll_max: 0.,
            dialog_scroll_max: 0.,
            last_error_generation: 0,
            seek_preview: None,
            roots: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            renderer: Default::default(),
            page: Page::Sonora,
            overlay: None,
            recognized: None,
            query: String::new(),
            submitted_query: String::new(),
            external_query: String::new(),
            search_filter: 0,
            library_selection: 0,
            selected_version: None,
            discovery: false,
            shuffle: false,
            muted_volume: 0.65,
            offsets: HashMap::new(),
            drag: None,
            carousel: Default::default(),
            last_root: None,
            page_from: None,
            rendered_page: Page::Sonora,
            page_started: -1.,
            rendered_overlay: None,
            overlay_started: -1.,
            last_message: String::new(),
            message_at: -99.,
            connection: Default::default(),
            yandex: String::new(),
            lastfm: String::new(),
            soundcloud: String::new(),
            reduced_motion: reduced_motion(),
        }
    }
    fn root(&self, id: &str) -> Node {
        self.roots[id].clone()
    }
    pub fn preview_page(&mut self, name: &str) {
        self.page = match name {
            "search" | "results" => Page::Search,
            "artist" => Page::Artist,
            "library" => Page::Library,
            "favorites" => Page::Favorites,
            "playlists" => Page::Playlists,
            "playlist" => Page::Playlist,
            _ => Page::Sonora,
        };
        self.overlay = match name {
            "updates" => Some(Overlay::Updates),
            #[cfg(windows)]
            "vps" => Some(Overlay::Vps),
            "queue" => Some(Overlay::Queue),
            "eq" | "eq-warm" => Some(Overlay::Eq),
            "sources" => Some(Overlay::Sources),
            "settings" => Some(Overlay::Settings),
            "cloud" => Some(Overlay::Cloud),
            "tune" => Some(Overlay::Tune),
            "recognition" => Some(Overlay::Recognition),
            "recognized" => Some(Overlay::Recognized),
            "credentials"
            | "credentials-lastfm"
            | "credentials-soundcloud"
            | "credentials-hitmo"
            | "credentials-youtube" => Some(Overlay::Credentials),
            "versions" => Some(Overlay::Versions),
            _ => None,
        };
        if self.overlay == Some(Overlay::Credentials) {
            let provider = match name {
                "credentials-lastfm" => Provider::LastFm,
                "credentials-soundcloud" => Provider::SoundCloud,
                "credentials-hitmo" => Provider::Hitmotop,
                "credentials-youtube" => Provider::YouTube,
                _ => Provider::Yandex,
            };
            self.connection.open(
                provider,
                if provider == Provider::Hitmotop {
                    crate::providers::connections::DEFAULT_HITMO.into()
                } else {
                    String::new()
                },
                0.,
            );
        }
    }
    fn current<'a>(&self, m: &'a Model) -> Option<&'a Track> {
        if m.resolving {
            return m
                .cursor
                .and_then(|i| m.queue.get(i))
                .or(m.status.track.as_ref());
        }
        m.status
            .track
            .as_ref()
            .or_else(|| m.cursor.and_then(|i| m.queue.get(i)))
    }
    fn source(track: &Track) -> String {
        track.cover.clone().unwrap_or_else(|| "missing".into())
    }
    fn track(&self, node: &mut Node, track: &Track, m: &Model) {
        node.set_image(&Self::source(track));
        if let Some(labels) = node.find_mut("Track labels") {
            labels.texts(&[&track.title, &track.artist]);
            let width = labels.width;
            for child in &mut labels.children {
                if child.kind == "TEXT" {
                    child.width = width;
                }
            }
        }
        if let Some(caption) = node.find_mut("Caption") {
            caption.texts(&[&track.title, &track.artist]);
            let width = caption.width;
            for text in &mut caption.children {
                if text.kind == "TEXT" {
                    text.move_to(0., text.y);
                    text.width = width;
                    text.text_align_horizontal = "CENTER".into();
                }
            }
        }
        let short = match track.provider {
            Provider::Yandex => "Яндекс",
            Provider::Hitmotop => "Hitmo",
            Provider::SoundCloud => "SoundCloud",
            Provider::YouTube => "YouTube",
            Provider::LastFm => "Last.fm",
        };
        let duration = if track.duration > 0 {
            model::duration(track.duration)
        } else {
            "—:—".into()
        };
        // Preserve the component's right-aligned metadata and 12px gaps when
        // source names and proportional duration digits change.
        let source_width = match track.provider {
            Provider::Yandex => 42.,
            Provider::Hitmotop => 33.,
            Provider::SoundCloud => 68.,
            Provider::YouTube => 48.,
            Provider::LastFm => 43.,
        };
        let digit_advances = [679., 365., 641., 630., 648., 624., 619., 596., 640., 619.];
        let time_width = if track.duration > 0 {
            (duration
                .chars()
                .map(|c| {
                    c.to_digit(10)
                        .map(|d| digit_advances[d as usize])
                        .unwrap_or(265.)
                })
                .sum::<f32>()
                * 0.012)
                .ceil()
        } else {
            30.
        };
        let metadata = node.children.iter().any(|n| n.kind == "TEXT" && n.visible);
        if metadata {
            let end = node
                .children
                .iter()
                .find(|n| n.name == "Save track")
                .map(|n| n.x - 12.)
                .unwrap_or(node.width - 64.);
            let time_x = end - time_width;
            let source_x = time_x - 12. - source_width;
            if let Some(labels) = node.find_mut("Track labels") {
                labels.width = (source_x - 12. - labels.x).max(60.);
                let width = labels.width;
                for text in &mut labels.children {
                    if text.kind == "TEXT" {
                        text.width = width;
                    }
                }
            }
            for text in &mut node.children {
                if text.kind == "TEXT" {
                    let (x, width) = if text.name == "Яндекс" {
                        (source_x, source_width)
                    } else {
                        (time_x, time_width)
                    };
                    text.move_to(x, text.y);
                    text.width = width;
                    text.natural_width = width;
                }
            }
        }
        for child in &mut node.children {
            if child.kind == "TEXT" {
                if child.name == "Яндекс" {
                    child.characters = short.into();
                } else {
                    child.characters = duration.clone();
                }
                child.original_characters = child.characters.clone();
            }
        }
        if let Some(like) = node.find_mut("Save track") {
            like.command = Some(Control::Like(track.clone()));
            if !m.preview && m.favorites.iter().any(|t| t.key() == track.key()) {
                Self::tint(
                    like,
                    serde_json::json!({"r":232.0/255.0,"g":1.0,"b":163.0/255.0}),
                );
            }
        }
    }
    fn tint(node: &mut Node, color: serde_json::Value) {
        node.visit(&mut |n| {
            if n.asset.is_some() {
                n.tint = color.clone();
            }
        });
    }
    fn command(node: &mut Node, name: &str, control: Control) {
        if let Some(n) = node.find_mut(name) {
            n.command = Some(control);
        }
    }
    fn slider(node: &mut Node, value: f32) {
        let width = node.width;
        if let Some(n) = node.find_mut("Progress") {
            n.width = width * value.clamp(0., 1.);
        }
        if let Some(n) = node.find_mut("Thumb") {
            n.move_to(width * value.clamp(0., 1.) - n.width / 2., n.y);
        }
    }
    fn scaffold(&mut self, root: &mut Node, m: &Model) {
        for (label, page) in [
            ("Sonora", Page::Sonora),
            ("Поиск", Page::Search),
            ("Медиатека", Page::Library),
        ] {
            Self::command(root, &format!("nav/{label}"), Control::Navigate(page));
        }
        Self::command(
            root,
            "button/Настройки",
            Control::Overlay(Overlay::Settings),
        );
        Self::command(
            root,
            "button/Мой профиль",
            Control::Overlay(Overlay::Sources),
        );
        if let Some(profile) = root.find_mut("button/Мой профиль") {
            profile.visible = false;
        }
        if let Some(avatar) = root.find_mut("Avatar") {
            avatar.visible = false;
        }
        // Header controls have their own identities, independent of player controls.
        for child in &mut root.children {
            match child.name.as_str() {
                "button/sliders-horizontal" => {
                    child.command = Some(Control::Overlay(if self.page == Page::Sonora {
                        Overlay::Tune
                    } else {
                        Overlay::Settings
                    }))
                }
                "button/list-music" => child.command = Some(Control::Overlay(Overlay::Queue)),
                _ => {}
            }
        }
        if let Some(account) = root.find_mut("Yandex connected") {
            account.name = "Connected providers".into();
            account.children.clear();
            account.command = Some(Control::Overlay(Overlay::Sources));
            for (connected, asset) in m.connected.into_iter().zip([
                "provider-yandex",
                "provider-lastfm",
                "provider-soundcloud",
                "provider-hitmo",
                "provider-youtube",
            ]) {
                if connected {
                    let mut logo = Node {
                        name: asset.into(),
                        asset: Some(asset.into()),
                        kind: "RECTANGLE".into(),
                        visible: true,
                        opacity: 1.,
                        width: 18.,
                        height: 18.,
                        command: Some(Control::Source(connections::label_for_asset(asset).into())),
                        ..Default::default()
                    };
                    logo.move_to(account.children.len() as f32 * 28., 0.);
                    account.children.push(logo);
                }
            }
        }
        if let Some(avatar) = root.find_mut("Avatar") {
            let initial = m.account.chars().next().unwrap_or('А').to_string();
            avatar.texts(&[&initial]);
        }
        if let Some(dock) = root.find_mut("Floating player") {
            let mut repeat = panels::icon_button(
                "repeat",
                0.,
                0.,
                32.,
                Control::Repeat,
                m.settings.repeat_current,
            );
            repeat.name = "Repeat".into();
            if !m.settings.repeat_current {
                repeat.fills.clear();
            }
            dock.children.push(repeat);
            if let Some(template) = dock.find("button/list-music") {
                let mut toggle = template.clone();
                toggle.name = "Lyrics toggle".into();
                toggle.command = Some(Control::ToggleLyrics);
                if m.settings.lyrics_enabled {
                    toggle.fills = vec![panels::fill([0.91, 1., 0.64], 1.)];
                    toggle.corner_radius = 16.;
                }
                toggle.visit(&mut |n| {
                    if n.asset.is_some() {
                        n.asset = Some("lyrics".into());
                    }
                });
                dock.children.push(toggle);
            }
            let current = self.current(m);
            if let Some(n) = dock.find_mut("Current artwork") {
                n.command = Some(Control::Zen(true));
                n.set_image(
                    &current
                        .map(Self::source)
                        .unwrap_or_else(|| "missing".into()),
                );
            }
            if let Some(n) = dock.find_mut("Now playing labels") {
                n.texts(&[
                    current
                        .map(|t| t.title.as_str())
                        .unwrap_or("Выберите музыку"),
                    current
                        .map(|t| t.artist.as_str())
                        .unwrap_or("Ваш следующий любимый трек"),
                ]);
                let w = n.width;
                for c in &mut n.children {
                    if c.kind == "TEXT" {
                        c.width = w;
                    }
                }
                if let Some(track) = current {
                    n.command = Some(Control::Row(track.clone()));
                }
            }
            if let Some(track) = current {
                if let Some(n) = dock.find_mut("Save current track") {
                    *n = panels::like_button(track, m, 0., 0., 44.);
                    n.name = "Save current track".into();
                }
            }
            Self::command(dock, "button/skip-back", Control::Previous);
            Self::command(dock, "button/skip-forward", Control::Next);
            Self::command(dock, "Play / pause", Control::PlayPause);
            Self::command(dock, "Shuffle", Control::Shuffle);
            Self::command(dock, "Mute", Control::Mute);
            Self::command(dock, "Volume", Control::Volume);
            Self::command(dock, "button/list-music", Control::Overlay(Overlay::Queue));
            Self::command(
                dock,
                "button/sliders-horizontal",
                Control::Overlay(Overlay::Eq),
            );
            if let Some(n) = dock.find_mut("Play / pause") {
                n.visit(&mut |c| {
                    if c.asset.is_some() {
                        c.asset = Some(if m.status.playing { "pause" } else { "play" }.into());
                    }
                });
            }
            if let Some(n) = dock.find_mut("Mute") {
                n.visit(&mut |c| {
                    if c.asset.is_some() {
                        c.asset = Some(
                            if m.settings.volume <= 0.001 {
                                "volume-x"
                            } else {
                                "volume-2"
                            }
                            .into(),
                        );
                    }
                });
            }
            if let Some(n) = dock.find_mut("Shuffle") {
                if self.shuffle {
                    n.fills = vec![panels::fill([0.91, 1., 0.64], 1.)];
                    n.corner_radius = 16.;
                }
            }
            if let Some(n) = dock.find_mut("Volume") {
                Self::slider(n, m.settings.volume);
            }
            if let Some(timeline) = dock.find_mut("Timeline") {
                let duration = if m.status.duration > 0. {
                    m.status.duration
                } else {
                    current.map(|t| t.duration as f32).unwrap_or(0.)
                };
                if let Some(n) = timeline.find_mut("Seek") {
                    Self::slider(
                        n,
                        if duration > 0. {
                            self.seek_preview.unwrap_or(m.status.position) / duration
                        } else {
                            0.
                        },
                    );
                    n.command = Some(Control::Seek);
                }
                if let Some(n) = timeline.find_mut("Time labels") {
                    n.texts(&[
                        &model::duration(
                            self.seek_preview.unwrap_or(m.status.position).max(0.) as u64
                        ),
                        &model::duration(duration as u64),
                    ]);
                }
            }
        }
        if let Some(panel) = root.find_mut("Now playing panel") {
            let current = self.current(m);
            if let Some(n) = panel.find_mut("Panel artwork") {
                n.set_image(
                    &current
                        .map(Self::source)
                        .unwrap_or_else(|| "missing".into()),
                );
            }
            if let Some(n) = panel.find_mut("Track title") {
                n.texts(&[
                    current.map(|t| t.title.as_str()).unwrap_or("Пока тихо"),
                    current
                        .map(|t| t.artist.as_str())
                        .unwrap_or("Выберите трек"),
                ]);
                for c in &mut n.children {
                    if c.kind == "TEXT" {
                        c.width = 228.;
                    }
                }
            }
            Self::command(panel, "button/list-music", Control::Overlay(Overlay::Queue));
            Self::command(
                panel,
                "button/ellipsis",
                current
                    .map(|t| Control::Row(t.clone()))
                    .unwrap_or(Control::Overlay(Overlay::Queue)),
            );
            let rows: Vec<Node> = panel
                .children
                .iter()
                .filter(|n| n.name.starts_with("track/"))
                .cloned()
                .collect();
            panel.children.retain(|n| !n.name.starts_with("track/"));
            let next = m.cursor.map(|i| i + 1).unwrap_or(0);
            let next_tracks: Vec<(usize, &Track)> = if m.preview {
                [1, 2, 5]
                    .into_iter()
                    .filter_map(|i| m.queue.get(i).map(|t| (i, t)))
                    .collect()
            } else {
                m.queue.iter().enumerate().skip(next).take(3).collect()
            };
            for (position, (index, track)) in next_tracks.into_iter().enumerate() {
                if let Some(template) = rows.get(position) {
                    let mut row = template.clone();
                    self.track(&mut row, track, m);
                    row.command = Some(Control::QueueIndex(index));
                    panel.children.push(row);
                }
            }
        }
    }
    fn row_template(&self) -> Node {
        self.roots["10:1123"]
            .children
            .iter()
            .find(|n| n.name.starts_with("track/"))
            .expect("Figma row component")
            .clone()
    }
    fn list(
        &mut self,
        ctx: &egui::Context,
        root: &mut Node,
        m: &Model,
        tracks: &[Track],
        viewport: ListViewport,
        inverse: bool,
    ) {
        let ListViewport { rect, key } = viewport;
        let rect = self.layout.list_rect(key, rect);
        let mut group = Node {
            id: format!("list/{key}"),
            name: format!("list/{key}"),
            kind: "FRAME".into(),
            visible: true,
            opacity: 1.,
            width: rect.width(),
            height: rect.height(),
            clips_content: true,
            ..Default::default()
        };
        group.move_to(rect.min.x, rect.min.y);
        let offset = self.offsets.entry(key.into()).or_default();
        let max = (tracks.len() as f32 * 68. - rect.height()).max(0.);
        let window = self
            .renderer
            .regions
            .get(&format!("list/{key}"))
            .map(|(bounds, clip)| bounds.intersect(*clip))
            .unwrap_or(rect);
        if self.overlay.is_none()
            && !(m.settings.lyrics_enabled
                && self
                    .renderer
                    .regions
                    .get("Lyrics content")
                    .is_some_and(|(bounds, clip)| {
                        ctx.input(|i| {
                            i.pointer
                                .hover_pos()
                                .is_some_and(|p| bounds.intersect(*clip).contains(p))
                        })
                    }))
            && ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)))
            && ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| window.contains(p)))
        {
            *offset = (*offset - (ctx.input(|i| i.smooth_scroll_delta.y) + self.touch_delta))
                .clamp(0., max);
        }
        *offset = offset.clamp(0., max);
        let offset = *offset;
        let template = self.row_template();
        for (i, track) in tracks.iter().enumerate() {
            let y = i as f32 * 68. - offset;
            if y + 68. < 0. || y > rect.height() {
                continue;
            }
            let mut row = template.clone();
            row.id = format!("{key}/{}", track.key());
            row.width = rect.width();
            row.move_to(0., y);
            if let Some(save) = row.find_mut("Save track") {
                save.move_to(rect.width() - 44., save.y);
            }
            if rect.width() < 500. {
                for child in &mut row.children {
                    if child.kind == "TEXT" {
                        child.visible = false;
                    }
                }
                if let Some(labels) = row.find_mut("Track labels") {
                    labels.width = (rect.width() - 120.).max(60.);
                }
            }
            if inverse {
                row.visit(&mut |n| {
                    if n.kind == "TEXT" {
                        for f in &mut n.fills {
                            f["color"] = serde_json::json!({"r":0.98,"g":0.973,"b":0.96});
                        }
                    }
                });
            }
            self.track(&mut row, track, m);
            row.command = Some(if key == "artist" {
                Control::ArtistTrack(i)
            } else {
                Control::Play(track.clone())
            });
            group.children.push(row);
        }
        if tracks.is_empty() {
            group.children.push(Node::text_node(
                if m.searching {
                    "Ищем вашу музыку…"
                } else if m.loading_playlist || m.loading_library {
                    "Загружаем вашу музыку…"
                } else {
                    "Здесь будет ваша музыка"
                },
                12.,
                20.,
                rect.width() - 24.,
                inverse,
            ));
        }
        if max > 0. {
            let thumb = (rect.height() * rect.height() / (tracks.len() as f32 * 68.))
                .max(24.)
                .min(rect.height());
            let mut rail = panels::frame(
                "Track scroll indicator",
                rect.width() - 3.,
                (rect.height() - thumb) * offset / max,
                3.,
                thumb,
            );
            rail.corner_radius = 1.5;
            rail.fills = vec![panels::fill([0.5, 0.48, 0.53], 0.35)];
            group.children.push(rail);
        }
        root.children.push(group);
    }
    fn scroll_offset(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        key: &str,
        total: f32,
        in_overlay: bool,
    ) -> f32 {
        let name = if key == "versions" {
            "Version list"
        } else {
            key
        };
        let window = self
            .renderer
            .regions
            .get(name)
            .map(|(bounds, clip)| bounds.intersect(*clip))
            .unwrap_or(rect);
        let offset = self.offsets.entry(key.into()).or_default();
        if self.overlay.is_some() == in_overlay
            && ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| window.contains(p)))
        {
            *offset -= ctx.input(|i| i.smooth_scroll_delta.y) + self.touch_delta;
        }
        *offset = offset.clamp(0., (total - rect.height()).max(0.));
        *offset
    }
    fn compose(&mut self, ctx: &egui::Context, m: &Model) -> Node {
        let results = !self.query.trim().is_empty()
            && (!self.submitted_query.is_empty() || m.searching || !m.results.is_empty());
        let id = match self.page {
            Page::Sonora => "8:157",
            Page::Search => {
                if results {
                    "10:604"
                } else {
                    "10:335"
                }
            }
            Page::Artist => "10:604",
            Page::Library => "10:877",
            Page::Favorites => "10:1123",
            Page::Playlists => "12:1722",
            Page::Playlist => "10:1413",
        };
        let mut root = self.root(id);
        if let Some(ambient) = self.roots["8:157"].find("Ambient light") {
            root.children.retain(|n| n.name != "Ambient light");
            root.children.insert(0, ambient.clone());
        }
        self.scaffold(&mut root, m);
        match self.page {
            Page::Sonora => {
                let fallback = demo_tracks();
                let tracks = if m.queue.is_empty() {
                    if m.favorites.is_empty() {
                        &fallback
                    } else {
                        &m.favorites
                    }
                } else {
                    &m.queue
                };
                let index = m.cursor.unwrap_or(0).min(tracks.len().saturating_sub(1));
                if let Some(stage) = root.find_mut("Cover flow · swipe") {
                    let canonical = stage.children.last().cloned();
                    for (card, offset) in stage.children.iter_mut().zip([-2, 2, -1, 1, 0]) {
                        if let Some(template) = &canonical {
                            *card = template.clone();
                        }
                        card.opacity = 1.;
                        if tracks.is_empty() {
                            card.visible = false;
                            continue;
                        }
                        let desired = index as isize + offset;
                        if !m.preview
                            && !m.queue.is_empty()
                            && (desired < 0 || desired >= tracks.len() as isize)
                        {
                            card.visible = false;
                            continue;
                        }
                        let i = desired.rem_euclid(tracks.len() as isize) as usize;
                        self.track(card, &tracks[i], m);
                        card.name = if m.queue.is_empty() {
                            format!("Album/{}", tracks[i].identity())
                        } else {
                            format!("Album/{i}")
                        };
                        card.command = Some(if !m.queue.is_empty() {
                            Control::QueueIndex(i)
                        } else {
                            Control::Play(tracks[i].clone())
                        });
                    }
                    if !m.sonora_ready {
                        if let Some(template) = canonical {
                            stage.children = vec![sonora::start_card(template, m)];
                        }
                    }
                }
                root.children.retain(|n| {
                    !matches!(
                        n.name.as_str(),
                        "pill/ПОТОК ДЛЯ ВАС" | "pill/Больше открытий"
                    )
                });
                let mut reset =
                    panels::icon_button("refresh-cw", 0., 0., 40., Control::ResetRadio, false);
                reset.name = "Reset Sonora".into();
                reset.fills.clear();
                root.children.push(reset);
                if let Some(reason) = root.find_mut("Recommendation reason") {
                    reason.visible = m.sonora_ready;
                    let width =
                        self.layout.size.x - self.layout.margin - self.layout.surface_x - 32.;
                    reason.children.clear();
                    if m.settings.last_artist.trim().is_empty()
                        || m.settings.last_title.trim().is_empty()
                    {
                        let mut headline =
                            panels::text("Похоже на Вашу музыку", 0., 12., width, 14., false);
                        headline.text_align_horizontal = "CENTER".into();
                        reason.children.push(headline);
                    } else {
                        let basis = m.settings.last_title.trim();
                        let prefix = "Подбор на основе";
                        let font_size = if width < 340. { 10. } else { 13. };
                        let prefix_w = ctx.fonts(|f| {
                            f.layout_no_wrap(
                                prefix.into(),
                                FontId::new(
                                    font_size * 2.,
                                    FontFamily::Name("medeo-medium".into()),
                                ),
                                Color32::BLACK,
                            )
                            .size()
                            .x / 2.
                        }) + 8.;
                        let title_w = ctx
                            .fonts(|f| {
                                f.layout_no_wrap(
                                    basis.into(),
                                    FontId::new(
                                        font_size * 2.,
                                        FontFamily::Name("medeo-semibold".into()),
                                    ),
                                    Color32::BLACK,
                                )
                                .size()
                                .x / 2.
                                    + 4.
                            })
                            .min((width - prefix_w).max(24.));
                        let x = ((width - prefix_w - title_w) / 2.).max(0.);
                        reason.children.extend([
                            panels::text(prefix, x, 12., prefix_w, font_size, false),
                            panels::text(basis, x + prefix_w, 12., title_w, font_size, true),
                        ]);
                    }
                }
            }
            Page::Search | Page::Artist => search::compose(self, ctx, m, &mut root, results),
            Page::Library | Page::Favorites | Page::Playlists => {
                root.children.retain(|n| {
                    !(n.kind == "TEXT"
                        && n.characters.contains("SoundCloud")
                        && n.characters.contains("YouTube Music"))
                });
                if let Some(tabs) = root.find_mut("Library filters") {
                    for (n, page) in tabs.children.iter_mut().zip([
                        Page::Library,
                        Page::Favorites,
                        Page::Playlists,
                    ]) {
                        n.command = Some(Control::Navigate(page));
                    }
                }
                Self::command(&mut root, "button/Слушать", Control::Listen(false));
                Self::command(&mut root, "button/Слушать всё", Control::Listen(false));
                Self::command(&mut root, "button/Перемешать", Control::Listen(true));
                Self::command(&mut root, "button/refresh-cw", Control::Sync);
                if self.page == Page::Favorites {
                    root.children.retain(|n| !n.name.starts_with("track/"));
                    if let Some(n) = root.find_mut("6 треков · ваша общая коллекция")
                    {
                        n.characters =
                            format!("{} треков · ваша общая коллекция", m.favorites.len());
                        n.width = 700.;
                    }
                    self.list(
                        ctx,
                        &mut root,
                        m,
                        &m.favorites,
                        ListViewport {
                            rect: Rect::from_min_size(pos2(264., 334.), vec2(786., 422.)),
                            key: "favorites",
                        },
                        false,
                    );
                } else {
                    if let Some(hero) = root.find_mut("Favorite collection") {
                        hero.texts(&[
                            "Любимое — всегда рядом.",
                            &format!("{} треков из разных музыкальных миров", m.favorites.len()),
                            "Слушать",
                        ]);
                        if !m.preview {
                            if let Some(t) = m.favorites.first() {
                                hero.set_image(&Self::source(t));
                            }
                        }
                        hero.command = Some(Control::Navigate(Page::Favorites));
                    }
                    for container_name in ["Playlist grid", "All playlists"] {
                        if let Some(grid) = root.find_mut(container_name) {
                            let templates = grid.children.clone();
                            grid.children.clear();
                            for (i, p) in m
                                .playlists
                                .iter()
                                .take(if self.page == Page::Library {
                                    4
                                } else {
                                    usize::MAX
                                })
                                .enumerate()
                            {
                                let y = (i / 4) as f32 * 266.;
                                if let Some(template) = templates.get(i % templates.len().max(1)) {
                                    let mut card = template.clone();
                                    card.id = format!("playlist/{i}");
                                    card.move_to((i % 4) as f32 * 200., y);
                                    if !m.preview {
                                        card.texts(&[
                                            &p.title,
                                            &format!("{} треков · Яндекс", p.count),
                                        ]);
                                        card.set_image(p.cover.as_deref().unwrap_or("missing"));
                                        card.visit(&mut |n| {
                                            if n.kind == "TEXT" {
                                                n.width = 176.;
                                            }
                                        });
                                    }
                                    card.command = Some(Control::Playlist(i));
                                    grid.children.push(card);
                                }
                            }
                            if m.playlists.is_empty() {
                                grid.children.push(Node::text_node(
                                    if m.loading_library {
                                        "Загружаем плейлисты…"
                                    } else {
                                        "Подключите Яндекс Музыку в настройках"
                                    },
                                    0.,
                                    20.,
                                    720.,
                                    false,
                                ));
                            }
                            grid.clips_content = true;
                        }
                    }
                }
            }
            Page::Playlist => {
                Self::command(
                    &mut root,
                    "button/chevron-left",
                    Control::Navigate(Page::Playlists),
                );
                Self::command(&mut root, "button/Слушать", Control::Listen(false));
                Self::command(&mut root, "button/Перемешать", Control::Listen(true));
                if let Some(n) = root.find_mut("Ночной маршрут") {
                    n.characters = m.playlist_title.clone();
                    n.width = 570.;
                }
                if let Some(n) = root.find_mut("6 треков · из Яндекс Музыки") {
                    n.characters = format!("{} треков · из Яндекс Музыки", m.playlist_tracks.len());
                    n.width = 550.;
                }
                if let Some(n) = root.find_mut("Playlist artwork") {
                    n.set_image(
                        &m.playlist_tracks
                            .first()
                            .map(Self::source)
                            .unwrap_or_else(|| "missing".into()),
                    );
                }
                root.children.retain(|n| n.name != "Scrollable playlist");
                self.list(
                    ctx,
                    &mut root,
                    m,
                    &m.playlist_tracks,
                    ListViewport {
                        rect: Rect::from_min_size(pos2(264., 430.), vec2(786., 368.)),
                        key: "playlist",
                    },
                    false,
                );
            }
        }
        root
    }
    fn compose_overlay(&mut self, ctx: &egui::Context, m: &Model) -> Option<Node> {
        let overlay = self.overlay?;
        if overlay == Overlay::Updates {
            return Some(updates::panel(self, m));
        }
        #[cfg(windows)]
        if overlay == Overlay::Vps {
            return Some(vps::panel(self));
        }
        if overlay == Overlay::Devices {
            return Some(cloud::devices_panel(self));
        }
        if overlay == Overlay::Cloud {
            return Some(cloud::panel(self));
        }
        if overlay == Overlay::Credentials {
            return Some(connections::panel(self));
        }
        if overlay == Overlay::Artists {
            return Some(search::artists_panel(self, m));
        }
        if overlay == Overlay::Settings {
            return Some(panels::settings(self, m));
        }
        if overlay == Overlay::Eq {
            return Some(panels::equalizer(self, m));
        }
        let id = match overlay {
            Overlay::Updates => unreachable!(),
            #[cfg(windows)]
            Overlay::Vps => unreachable!(),
            Overlay::Cloud | Overlay::Devices => unreachable!(),
            Overlay::Artists => unreachable!(),
            Overlay::Settings => "13:2407",
            Overlay::Queue => "13:2146",
            Overlay::Eq => {
                if m.settings.eq == [4., 3., 2., 1., 0., -1., -1., 0., 1., 2.] {
                    "13:2290"
                } else {
                    "13:2221"
                }
            }
            Overlay::Tune => {
                if self.discovery {
                    "13:2383"
                } else {
                    "13:2359"
                }
            }
            Overlay::Sources => {
                if m.connected[2] {
                    "13:2460"
                } else {
                    "13:2407"
                }
            }
            Overlay::Credentials => "13:2407",
            Overlay::Recognition => "13:2513",
            Overlay::Recognized => "13:2533",
            Overlay::Versions => "13:2548",
        };
        let mut root = self.root(id);
        Self::command(&mut root, "Click outside to close", Control::Close);
        Self::command(&mut root, "Glass dialog", Control::Noop);
        Self::command(&mut root, "button/x", Control::Close);
        match overlay {
            Overlay::Updates => unreachable!(),
            #[cfg(windows)]
            Overlay::Vps => unreachable!(),
            Overlay::Cloud | Overlay::Devices => unreachable!(),
            Overlay::Artists => unreachable!(),
            Overlay::Settings => unreachable!(),
            Overlay::Queue => {
                if let Some(panel) = root.find_mut("Glass dialog") {
                    let templates: Vec<Node> = panel
                        .children
                        .iter()
                        .filter(|n| n.name.starts_with("track/"))
                        .cloned()
                        .collect();
                    panel.children.retain(|n| !n.name.starts_with("track/"));
                    if m.preview {
                        for (template, index) in templates.iter().zip([3, 0, 1, 2, 5, 4]) {
                            if let Some(track) = m.queue.get(index) {
                                let mut row = template.clone();
                                self.track(&mut row, track, m);
                                row.command = Some(Control::QueueIndex(index));
                                panel.children.push(row);
                            }
                        }
                    } else {
                        let current = m.cursor.unwrap_or(0).min(m.queue.len());
                        let past = (0..current).rev().collect::<Vec<_>>();
                        self.queue_list(
                            ctx,
                            panel,
                            m,
                            &templates[0],
                            &past,
                            ListViewport {
                                rect: Rect::from_min_size(pos2(28., 146.), vec2(316., 56.)),
                                key: "queue-past",
                            },
                        );
                        if let Some(index) = m.cursor {
                            if let Some(track) = m.queue.get(index) {
                                let mut row = templates[1].clone();
                                self.track(&mut row, track, m);
                                row.command = Some(Control::QueueIndex(index));
                                panel.children.push(row);
                            }
                        }
                        let next = m.cursor.map(|i| i + 1).unwrap_or(0);
                        let future = (next..m.queue.len()).collect::<Vec<_>>();
                        self.queue_list(
                            ctx,
                            panel,
                            m,
                            &templates[2],
                            &future,
                            ListViewport {
                                rect: Rect::from_min_size(pos2(28., 374.), vec2(316., 264.)),
                                key: "queue-next",
                            },
                        );
                    }
                    if m.queue.is_empty() {
                        panel.children.push(Node::text_node(
                            "Запустите поток или выберите трек",
                            28.,
                            270.,
                            316.,
                            true,
                        ));
                    }
                }
            }
            Overlay::Eq => unreachable!(),
            Overlay::Tune => {
                Self::command(
                    &mut root,
                    "Choice/Ближе к любимому",
                    Control::Discovery(false),
                );
                Self::command(
                    &mut root,
                    "Choice/Больше открытий",
                    Control::Discovery(true),
                );
                Self::command(&mut root, "button/Применить", Control::ApplyTune);
            }
            Overlay::Sources => {
                for (name, asset) in [
                    ("Яндекс Музыка", "provider-yandex"),
                    ("Last.fm", "provider-lastfm"),
                    ("SoundCloud", "provider-soundcloud"),
                    ("Hitmo", "provider-hitmo"),
                    ("YouTube Music", "provider-youtube"),
                ] {
                    if let Some(mark) = root
                        .find_mut(&format!("Source/{name}"))
                        .and_then(|n| n.find_mut("Source mark"))
                    {
                        mark.children.clear();
                        mark.fills.clear();
                        mark.asset = Some(asset.into());
                        mark.tint = serde_json::Value::Null;
                    }
                }
                for (source, connected, status) in [
                    (
                        "Яндекс Музыка",
                        m.connected[0],
                        if m.account.is_empty() {
                            "Подключён · подписка Плюс"
                        } else {
                            &m.account
                        },
                    ),
                    ("Last.fm", m.connected[1], "Подборки активны"),
                    ("SoundCloud", m.connected[2], "Подключён"),
                    ("Hitmo", m.connected[3], "Домен настраивается"),
                    ("YouTube Music", m.connected[4], "Поиск без API-ключа"),
                ] {
                    if let Some(row) = root.find_mut(&format!("Source/{source}")) {
                        row.command = Some(Control::Source(source.into()));
                        if let Some(labels) = row.find_mut("Source labels") {
                            labels.texts(&[
                                source,
                                if connected {
                                    status
                                } else {
                                    "Не подключён"
                                },
                            ]);
                        }
                        if !connected {
                            for c in &mut row.children {
                                if c.name.starts_with("icon/check") {
                                    c.visible = false;
                                }
                            }
                        }
                    }
                }
                Self::command(
                    &mut root,
                    "button/Подключить",
                    Control::Source("SoundCloud".into()),
                );
                Self::command(&mut root, "button/Готово", Control::Close);
                if let Some(n) =
                    root.find_mut("В прототипе подключение аккаунтов демонстрационное.")
                {
                    n.characters = if crate::platform::mobile() {
                        "Ключи защищены Android Keystore."
                    } else {
                        "Ключи хранятся в учётных данных Windows."
                    }
                    .into();
                    n.width = 550.;
                }
                if let Some(row) = root.find_mut("Source/YouTube Music") {
                    row.texts(&["YouTube Music", "Поиск и доступные записи"]);
                    if let Some(labels) = row.find_mut("Source labels") {
                        for text in &mut labels.children {
                            if text.kind == "TEXT" {
                                text.width = 282.;
                            }
                        }
                    }
                }
            }
            Overlay::Credentials => unreachable!(),
            Overlay::Recognition => {
                Self::command(&mut root, "button/Отменить", Control::Close);
            }
            Overlay::Recognized => {
                if let Some(track) = &self.recognized {
                    if let Some(n) = root.find_mut("Recognized artwork") {
                        n.set_image(&Self::source(track));
                    }
                    if let Some(n) = root.find_mut("Blinding Lights") {
                        n.characters = track.title.clone();
                        n.move_to(28., 284.);
                        n.width = 464.;
                        n.text_align_horizontal = "CENTER".into();
                    }
                    if let Some(n) = root.find_mut("The Weeknd") {
                        n.characters = track.artist.clone();
                        n.move_to(28., 324.);
                        n.width = 464.;
                        n.text_align_horizontal = "CENTER".into();
                    }
                }
                Self::command(
                    &mut root,
                    "button/Найти в каталогах",
                    Control::RecognizedSearch,
                );
            }
            Overlay::Versions => {
                let alternatives = m.alternatives.as_ref();
                if let Some(n) = root.find_mut("Текущий трек") {
                    n.characters = alternatives
                        .map(|a| format!("{} — {}", a.0.artist, a.0.title))
                        .unwrap_or_default();
                    n.width = 560.;
                }
                if let Some(panel) = root.find_mut("Glass dialog") {
                    let templates: Vec<Node> = panel
                        .children
                        .iter()
                        .filter(|n| n.name.starts_with("Version/"))
                        .cloned()
                        .collect();
                    panel.children.retain(|n| !n.name.starts_with("Version/"));
                    if let Some((_, tracks, _)) = alternatives {
                        let offset = self.scroll_offset(
                            ctx,
                            Rect::from_min_size(pos2(438., 366.), vec2(564., 282.)),
                            "versions",
                            tracks.len() as f32 * 94.,
                            true,
                        );
                        let mut group = Node {
                            id: "version-list".into(),
                            name: "Version list".into(),
                            kind: "FRAME".into(),
                            visible: true,
                            opacity: 1.,
                            width: 564.,
                            height: 282.,
                            clips_content: true,
                            ..Default::default()
                        };
                        group.move_to(28., 154.);
                        for (i, track) in tracks.iter().enumerate() {
                            let y = i as f32 * 94. - offset;
                            if y + 82. < 0. || y > 282. {
                                continue;
                            }
                            let mut row = templates[i % templates.len()].clone();
                            row.move_to(0., y);
                            if let Some(n) = row.find_mut("Version labels") {
                                if !track.page_url.is_empty() {
                                    n.command = Some(Control::Open(track.page_url.clone()));
                                }
                                n.texts(&[
                                    track.provider.label(),
                                    if self.selected_version == Some(i) {
                                        "Оригинал · выбрано вами"
                                    } else {
                                        track.edition.label()
                                    },
                                ]);
                            }
                            Self::command(&mut row, "Preview version", Control::VersionPreview(i));
                            if let Some(n) = row.children.last_mut() {
                                n.command = Some(Control::VersionSelect(i));
                                if self.selected_version == Some(i) {
                                    n.visit(&mut |c| {
                                        if c.asset.is_some() {
                                            c.asset = Some("check".into());
                                            c.tint = serde_json::json!({"r":232.0/255.0,"g":1.0,"b":163.0/255.0});
                                        }
                                    });
                                }
                            }
                            if self.selected_version == Some(i) {
                                row.fills = self.roots["13:2597"]
                                    .find("Version/Hitmo")
                                    .unwrap()
                                    .fills
                                    .clone();
                            }
                            group.children.push(row);
                        }
                        panel.children.push(group);
                        if tracks.is_empty() {
                            panel.children.push(Node::text_node(
                                if m.versions_loading {
                                    "Сопоставляем записи в каталогах…"
                                } else {
                                    "Другие версии пока не найдены"
                                },
                                28.,
                                175.,
                                560.,
                                true,
                            ));
                        }
                    }
                }
                Self::command(&mut root, "button/Оставить текущую", Control::VersionApply);
                if let Some(n) = root.find_mut("button/Оставить текущую") {
                    if self.selected_version.is_some() {
                        *n = self.roots["13:2597"]
                            .find("button/Сохранить выбор")
                            .unwrap()
                            .clone();
                        n.command = Some(Control::VersionApply);
                    }
                }
            }
        }
        Some(root)
    }
    fn queue_list(
        &mut self,
        ctx: &egui::Context,
        panel: &mut Node,
        m: &Model,
        template: &Node,
        indices: &[usize],
        viewport: ListViewport,
    ) {
        let ListViewport { rect, key } = viewport;
        let global = rect.translate(vec2(panel.x, panel.y));
        let offset = self.scroll_offset(ctx, global, key, indices.len() as f32 * 66., true);
        let mut group = Node {
            id: key.into(),
            name: key.into(),
            kind: "FRAME".into(),
            visible: true,
            opacity: 1.,
            width: rect.width(),
            height: rect.height(),
            clips_content: true,
            ..Default::default()
        };
        group.move_to(rect.min.x, rect.min.y);
        for (i, index) in indices.iter().enumerate() {
            let y = i as f32 * 66. - offset;
            if y + 56. < 0. || y > rect.height() {
                continue;
            }
            if let Some(track) = m.queue.get(*index) {
                let mut row = template.clone();
                row.id = format!("{key}/{index}");
                row.move_to(0., y);
                self.track(&mut row, track, m);
                row.command = Some(Control::QueueIndex(*index));
                group.children.push(row);
            }
        }
        panel.children.push(group);
    }
    fn process(
        &mut self,
        control: Control,
        m: &Model,
        out: &mut Vec<Intent>,
        local: egui::Pos2,
        size: egui::Vec2,
    ) {
        match control {
            Control::Update(action) => out.push(Intent::Update(action)),
            #[cfg(windows)]
            Control::VpsAuth(key) => self.vps_editor.key = key,
            #[cfg(windows)]
            Control::VpsStart(connect_only) => {
                match self
                    .vps_editor
                    .request(connect_only, &self.cloud_editor.name)
                {
                    Ok(request) => {
                        self.vps_editor.error = None;
                        out.push(Intent::Cloud(crate::cloud::Action::Provision(Box::new(
                            request,
                        ))));
                    }
                    Err(error) => self.vps_editor.error = Some(error.to_string()),
                }
            }
            Control::Cloud(action) => {
                if self.overlay == Some(Overlay::Devices)
                    && matches!(action, crate::cloud::Action::Target(_))
                {
                    self.overlay = None;
                }
                out.push(Intent::Cloud(action));
            }
            Control::CloudConnect => out.push(Intent::Cloud(crate::cloud::Action::Connect {
                invitation: self.cloud_editor.invitation.clone(),
                name: self.cloud_editor.name.clone(),
            })),
            Control::CloudCopy(browser) => self.cloud_editor.copy = Some(browser),
            Control::CloudLan => self.cloud_editor.lan = !self.cloud_editor.lan,
            Control::Artist(artist) => {
                self.page = Page::Artist;
                self.overlay = None;
                self.offsets.remove("artist");
                out.push(Intent::Artist(artist));
            }
            Control::ArtistMore => out.push(Intent::ArtistMore),
            Control::ArtistTrack(index) => {
                if let Some(catalog) = &m.artist {
                    out.push(Intent::PlayCollection(catalog.tracks.clone(), index));
                }
            }
            Control::ArtistSources(name) => {
                self.page = Page::Search;
                self.query = name.clone();
                self.submitted_query = name.clone();
                self.search_filter = 2;
                self.offsets.remove("results");
                out.push(Intent::Search(name));
            }
            Control::Noop => {}
            Control::ResetRadio => out.push(Intent::ResetRadio),
            Control::Theme(value) => {
                out.push(Intent::Theme(value));
            }
            Control::Navigate(page) => {
                self.page = page;
                self.overlay = None;
                out.push(Intent::Navigate(page));
            }
            Control::Overlay(overlay) => {
                if overlay == Overlay::Artists {
                    self.offsets.remove("dialog-scroll");
                }
                self.overlay = Some(overlay);
                self.discovery = m.settings.discovery >= 0.6;
            }
            Control::Close => {
                #[cfg(windows)]
                if self.overlay == Some(Overlay::Vps) {
                    self.vps_editor.clear_secrets();
                }
                if self.overlay == Some(Overlay::Recognition) {
                    out.push(Intent::CancelRecognition);
                }
                self.overlay = None;
            }
            Control::PlayPause => {
                if let Some(index) = m.cursor.filter(|_| m.status.ended) {
                    out.push(Intent::QueueIndex(index));
                } else if m.status.track.is_some() {
                    out.push(Intent::Pause);
                } else if self.page == Page::Sonora {
                    out.push(Intent::ResetRadio);
                } else if let Some(t) = m.results.first() {
                    out.push(Intent::Play(t.clone()));
                }
            }
            Control::Next => out.push(Intent::Next),
            Control::Previous => out.push(Intent::Previous),
            Control::Shuffle => {
                self.shuffle = !self.shuffle;
                out.push(Intent::Shuffle(self.shuffle));
            }
            Control::Mute => {
                if crate::platform::mobile() {
                    self.volume_until = if self.volume_until.is_infinite() {
                        -1.
                    } else {
                        f64::INFINITY
                    };
                    return;
                }
                if m.settings.volume > 0.001 {
                    self.muted_volume = m.settings.volume;
                    out.push(Intent::Volume(0.));
                } else {
                    out.push(Intent::Volume(self.muted_volume.max(0.1)));
                }
            }
            Control::Seek => {
                let duration = if m.status.duration > 0. {
                    m.status.duration
                } else {
                    self.current(m).map(|t| t.duration as f32).unwrap_or(0.)
                };
                if duration > 0. {
                    out.push(Intent::Seek((local.x / size.x).clamp(0., 1.) * duration));
                }
            }
            Control::Volume => out.push(Intent::Volume((local.x / size.x).clamp(0., 1.))),
            Control::EqBand(i) => {
                let mut gains = m.settings.eq;
                gains[i] = (12. - (local.y - 28.) / 160. * 24.)
                    .round()
                    .clamp(-12., 12.);
                out.push(Intent::Eq(gains));
            }
            Control::Preamp => out.push(Intent::Preamp(
                (12. - (local.y - 28.) / 160. * 24.)
                    .round()
                    .clamp(-12., 12.),
            )),
            Control::Preset(index) => out.push(Intent::Preset(index)),
            Control::Crossfade => {
                out.push(Intent::Crossfade((local.x / size.x * 12.).clamp(0., 12.)))
            }
            Control::CrossfadeOff => out.push(Intent::Crossfade(0.)),
            Control::CloseToTray(value) => out.push(Intent::CloseToTray(value)),
            Control::ToggleLyrics => out.push(Intent::Lyrics(!m.settings.lyrics_enabled)),
            Control::LyricSeek(time) => out.push(Intent::Seek(time)),
            Control::VerticalVolume => {
                out.push(Intent::Volume((1. - local.y / size.y).clamp(0., 1.)))
            }
            Control::Zen(value) => {
                self.zen = value;
                self.page_from = None;
                out.push(Intent::Zen(value));
            }
            Control::Repeat => out.push(Intent::Repeat(!m.settings.repeat_current)),
            Control::Discovery(value) => {
                self.discovery = value;
            }
            Control::ApplyTune => {
                out.push(Intent::Discovery(if self.discovery { 0.8 } else { 0.25 }));
                self.overlay = None;
            }
            Control::Like(track) => out.push(Intent::Like(track)),
            Control::Radio(track) => out.push(Intent::Radio(track)),
            Control::Play(track) => {
                let collection = match self.page {
                    Page::Favorites => Some(&m.favorites),
                    Page::Playlist => Some(&m.playlist_tracks),
                    _ => None,
                };
                if let Some((tracks, index)) = collection.and_then(|tracks| {
                    tracks
                        .iter()
                        .position(|t| t.key() == track.key())
                        .map(|index| (tracks, index))
                }) {
                    out.push(Intent::PlayCollection(tracks.clone(), index));
                } else if self.page == Page::Search {
                    out.push(Intent::Radio(track));
                } else {
                    out.push(Intent::Play(track));
                }
            }
            Control::Row(track) => {
                self.overlay = Some(Overlay::Versions);
                self.selected_version = None;
                out.push(Intent::Versions(track));
            }
            Control::QueueIndex(i) => {
                out.push(Intent::QueueIndex(i));
                if self.overlay == Some(Overlay::Queue) {
                    self.overlay = None;
                }
            }
            Control::Playlist(i) => {
                self.library_selection = i;
                self.page = Page::Playlist;
                out.push(Intent::Playlist(i));
            }
            Control::Mic => {
                self.overlay = Some(Overlay::Recognition);
                self.recognized = None;
                out.push(Intent::Recognize);
            }
            Control::Search => {
                if !self.query.trim().is_empty() {
                    self.submitted_query = self.query.clone();
                    out.push(Intent::Search(self.query.clone()));
                }
            }
            Control::Sync => out.push(Intent::Sync),
            Control::Listen(shuffle) => {
                let tracks = if self.page == Page::Playlist {
                    &m.playlist_tracks
                } else if self.page == Page::Artist {
                    if let Some(catalog) = &m.artist {
                        &catalog.tracks
                    } else {
                        return;
                    }
                } else {
                    &m.favorites
                };
                if !tracks.is_empty() {
                    let index = if shuffle {
                        use std::hash::{BuildHasher, Hasher};
                        let random = std::collections::hash_map::RandomState::new();
                        let mut h = random.build_hasher();
                        h.write_u64(m.status.ticket);
                        h.finish() as usize % tracks.len()
                    } else {
                        0
                    };
                    let mut ordered = tracks.clone();
                    ordered.rotate_left(index);
                    out.push(Intent::PlayCollection(ordered, 0));
                    self.shuffle = shuffle;
                    if shuffle {
                        self.shuffle = true;
                        out.push(Intent::Shuffle(true));
                    }
                }
            }
            Control::Source(source) => {
                if let Some(i) = source.strip_prefix("filter/").and_then(|s| s.parse().ok()) {
                    self.search_filter = i;
                    self.offsets.remove("results");
                } else {
                    let provider = connections::provider(&source);
                    let value = match provider {
                        Provider::Yandex => self.yandex.clone(),
                        Provider::LastFm => self.lastfm.clone(),
                        Provider::SoundCloud => self.soundcloud.clone(),
                        Provider::Hitmotop => m.settings.hitmo_base.clone(),
                        Provider::YouTube => String::new(),
                    };
                    self.connection.open(provider, value, 0.);
                    self.overlay = Some(Overlay::Credentials);
                }
            }
            Control::PreferOriginal => {
                out.push(Intent::PreferOriginal(!m.settings.prefer_original))
            }
            Control::SaveConnection => out.push(Intent::SaveConnection(
                self.connection.provider,
                self.connection.draft.clone(),
            )),
            Control::CheckConnection => self.connection.retry(),
            Control::VersionSelect(i) => self.selected_version = Some(i),
            Control::VersionPreview(i) => {
                if let Some((_, tracks, _)) = &m.alternatives {
                    if let Some(t) = tracks.get(i) {
                        out.push(Intent::Play(t.clone()));
                    }
                }
            }
            Control::VersionApply => {
                if let (Some(i), Some((original, tracks, _))) =
                    (self.selected_version, &m.alternatives)
                {
                    if let Some(track) = tracks.get(i) {
                        out.push(Intent::SaveVersion(original.clone(), track.clone()));
                    }
                } else if let Some((original, _, _)) = &m.alternatives {
                    out.push(Intent::ClearVersion(original.clone()));
                }
                self.overlay = None;
            }
            Control::RecognizedSearch => {
                if let Some(t) = &self.recognized {
                    self.query = t.query();
                    self.submitted_query = self.query.clone();
                    self.page = Page::Search;
                    out.push(Intent::Search(self.query.clone()));
                }
                self.overlay = None;
            }
            Control::Open(url) => out.push(Intent::Open(url)),
        }
    }
    fn input(
        &mut self,
        ui: &mut egui::Ui,
        _origin: egui::Pos2,
        scale: f32,
        out: &mut Vec<Intent>,
        m: &Model,
    ) {
        let ctx = ui.ctx().clone();
        if self.page == Page::Search && self.overlay.is_none() {
            let Some((field, clip)) = self.renderer.regions.get("Search field").copied() else {
                return;
            };
            let rect = Rect::from_min_size(
                field.min + vec2(44., 17.),
                vec2((field.width() - 104.).max(40.), 22.),
            );
            ui.set_clip_rect(clip);
            let id = egui::Id::new("music-query");
            crate::platform::text_before(&ctx, id, &mut self.query);
            let edit = egui::TextEdit::singleline(&mut self.query)
                .id(egui::Id::new("music-query"))
                .frame(false)
                .margin(0.)
                .font(FontId::new(
                    14. * scale,
                    FontFamily::Name("medeo-medium".into()),
                ))
                .text_color(
                    if ctx.memory(|memory| memory.has_focus(egui::Id::new("music-query"))) {
                        if self.dark {
                            Color32::from_rgb(242, 240, 246)
                        } else {
                            Color32::from_rgb(28, 26, 30)
                        }
                    } else {
                        Color32::TRANSPARENT
                    },
                )
                .desired_width(rect.width());
            let response = ui.put(rect, edit);
            crate::platform::text_after(&ctx, id, &self.query, false, true);
            if (response.has_focus() || response.lost_focus())
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !self.query.trim().is_empty()
            {
                self.submitted_query = self.query.clone();
                self.offsets.remove("results");
                out.push(Intent::Search(self.query.clone()));
            }
        }
        if self.overlay == Some(Overlay::Credentials) {
            connections::input(self, ui, out);
        }
        if self.overlay == Some(Overlay::Cloud) {
            cloud::input(self, ui);
        }
        #[cfg(windows)]
        if self.overlay == Some(Overlay::Vps) {
            vps::input(self, ui);
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.process(
                    Control::PlayPause,
                    m,
                    out,
                    egui::Pos2::ZERO,
                    egui::Vec2::ZERO,
                );
            }
            if self.overlay.is_none() {
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    out.push(Intent::Next);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    out.push(Intent::Previous);
                }
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.process(
                if self.overlay.is_some() {
                    Control::Close
                } else {
                    Control::Zen(false)
                },
                m,
                out,
                egui::Pos2::ZERO,
                egui::Vec2::ZERO,
            );
        }
    }
    pub fn show(&mut self, ctx: &egui::Context, m: &Model) -> Vec<Intent> {
        let mut out = vec![];
        self.touch_delta = 0.;
        if crate::platform::mobile() {
            if ctx.input(|i| i.pointer.primary_pressed()) {
                self.touch_start = ctx.input(|i| i.pointer.interact_pos().map(|p| (p, i.time)));
                self.touch_long = false;
            }
            if ctx.input(|i| i.pointer.primary_down()) && self.drag.is_none() {
                if let Some((start, _)) = self.touch_start {
                    if ctx.input(|i| {
                        i.pointer
                            .interact_pos()
                            .is_some_and(|p| p.distance(start) > 8.)
                    }) {
                        self.touch_delta = ctx.input(|i| i.pointer.delta().y);
                    }
                }
            }
        }
        let dark = m.settings.theme.dark(crate::platform::system_dark(ctx));
        if self.dark != dark {
            self.dark = dark;
            self.page_from = None;
            self.last_root = None;
        }
        self.renderer.preload_covers(
            ctx,
            m.queue
                .iter()
                .skip(m.cursor.unwrap_or(0).saturating_sub(2))
                .take(8)
                .chain(m.status.track.iter()),
        );
        let now = ctx.input(|i| i.time);
        let lyrics_track = m.status.track.as_ref().map(Track::key);
        if lyrics_track != self.lyrics_track {
            self.lyrics_manual_until = -1.;
            self.lyrics_manual_index = None;
            self.lyrics_wheel_remainder = 0.;
            self.lyrics_manual_visibility = transitions::Tween::new(0.);
            self.lyrics_fitted_font = None;
            self.lyrics_motion = Default::default();
            self.lyrics_emphasis.clear();
            self.lyrics_track = lyrics_track;
            self.offsets.remove("lyrics-live");
            self.offsets.remove("lyrics-scroll");
        }
        if self.external_query != m.query {
            self.external_query = m.query.clone();
            self.query = m.query.clone();
            self.submitted_query = m.query.clone();
        }
        if self.recognized.is_some() && self.overlay == Some(Overlay::Recognition) && !m.recording {
            self.overlay = Some(Overlay::Recognized);
        }
        if self.overlay == Some(Overlay::Versions) && m.alternatives.is_none() {
            self.overlay = None;
        }
        if (m.message != self.last_message || m.error_generation != self.last_error_generation)
            && self.overlay.is_none()
        {
            self.last_message = m.message.clone();
            self.last_error_generation = m.error_generation;
            self.message_at = now;
        }
        let rect = ctx.screen_rect();
        if self.layout.size != rect.size() {
            self.carousel = motion::Carousel::default();
            self.page_from = None;
            self.last_root = None;
        }
        self.layout = layout::Layout::for_page(rect.size(), self.page);
        let scale = 1.;
        let origin = rect.min;
        let transform = Transform::new(scale, origin);
        self.renderer.ambient_phase = if self.reduced_motion
            || (m.preview && std::env::var_os("WEDEX_PREVIEW_MOTION").is_none())
        {
            0.
        } else {
            motion::ambient(now)
        };
        let mut root = if self.zen {
            panels::zen(self, ctx, m, ctx.input(|i| i.pointer.hover_pos()))
        } else {
            self.compose(ctx, m)
        };
        let key = format!("page/{:?}", self.page);
        let scroll = *self.offsets.get(&key).unwrap_or(&0.);
        self.page_scroll_max = if self.zen {
            0.
        } else {
            self.layout.root(&mut root, self.page, scroll)
        };
        if !self.zen && !self.layout.mobile {
            if let Some(art) = root.find_mut("Current artwork") {
                let bounds = Rect::from_min_size(
                    pos2(self.layout.surface_x + art.x, self.layout.player_y + art.y),
                    vec2(art.width, art.height),
                );
                if ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| bounds.contains(p))) {
                    let mut shade = panels::frame("Expand preview", 0., 0., art.width, art.height);
                    shade.fills = vec![panels::fill([0., 0., 0.], 0.4)];
                    shade.corner_radius = art.corner_radius;
                    let mut icon = panels::icon(
                        "expand",
                        art.width * 0.24,
                        art.height * 0.24,
                        art.width * 0.52,
                    );
                    icon.tint = serde_json::json!({"r":1.,"g":1.,"b":1.});
                    shade.children.push(icon);
                    art.children.push(shade);
                    egui::show_tooltip_at_pointer(
                        ctx,
                        egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("expand-art")),
                        egui::Id::new("expand-art-tip"),
                        |ui| {
                            ui.label("На весь экран");
                        },
                    );
                }
            }
        }
        theme::apply(&mut root, self.dark);
        self.buttons.apply(
            &mut root,
            ctx.input(|i| i.pointer.hover_pos()),
            ctx.input(|i| i.pointer.primary_down()),
            now,
            self.reduced_motion,
            self.dark,
        );
        if let Some(stage) = root.find_mut("Cover flow · swipe") {
            self.carousel
                .apply(&mut stage.children, now, self.reduced_motion);
        }
        if self.rendered_page != self.page {
            self.page_from = self.last_root.clone();
            self.page_started = now;
            self.rendered_page = self.page;
        }
        self.last_root = Some(root.clone());
        let page_progress = if self.reduced_motion || self.page_from.is_none() {
            1.
        } else {
            motion::ease(((now - self.page_started) / transitions::DURATION).clamp(0., 1.) as f32)
        };
        let previous = if page_progress < 0.999 {
            self.page_from.clone()
        } else {
            self.page_from = None;
            None
        };
        if previous.is_some() {
            root.opacity = page_progress.max(0.0001);
        }
        if self.rendered_overlay != self.overlay {
            if self.overlay.is_some() {
                self.overlay_started = now;
                self.closing_overlay = None;
            } else {
                self.closing_overlay = self.last_overlay.take();
                self.overlay_closed_at = now;
                self.overlay_started = -1.;
            }
            self.rendered_overlay = self.overlay;
        }
        let mut overlay = self.compose_overlay(ctx, m);
        if let Some(root) = &mut overlay {
            if root.name != "Runtime panel" {
                self.dialog_scroll_max = self
                    .layout
                    .overlay(root, *self.offsets.get("dialog-scroll").unwrap_or(&0.));
            }
            theme::apply(root, self.dark);
            self.buttons.apply(
                root,
                ctx.input(|i| i.pointer.hover_pos()),
                ctx.input(|i| i.pointer.primary_down()),
                now,
                self.reduced_motion,
                self.dark,
            );
        }
        if !self.reduced_motion && !m.preview {
            if let Some(root) = &mut overlay {
                if self.overlay_started >= 0. {
                    root.opacity = motion::ease_out(
                        ((now - self.overlay_started) / transitions::DURATION).clamp(0., 1.) as f32,
                    )
                    .max(0.0001);
                }
            }
        }
        self.last_overlay = overlay.clone();
        let mut closing = self.closing_overlay.clone();
        if let Some(root) = &mut closing {
            let progress =
                ((now - self.overlay_closed_at) / transitions::DURATION).clamp(0., 1.) as f32;
            if self.reduced_motion || progress >= 1. {
                closing = None;
                self.closing_overlay = None;
            } else {
                root.opacity *= 1. - motion::ease_out(progress);
            }
        }
        self.renderer.begin();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    rect,
                    0.,
                    if self.dark {
                        Color32::from_rgb(14, 12, 18)
                    } else {
                        Color32::from_rgb(249, 248, 246)
                    },
                );
                if let Some(previous) = &previous {
                    self.renderer
                        .paint(ui.painter(), previous, transform, 1., true, false);
                }
                self.renderer
                    .paint(ui.painter(), &root, transform, 1., true, overlay.is_none());
                if overlay.is_none() {
                    if !self.zen {
                        let opacity = self.lyrics_visibility.value(
                            if m.settings.lyrics_enabled { 1. } else { 0. },
                            now,
                            self.reduced_motion,
                        );
                        if m.settings.lyrics_enabled {
                            let mut lyrics = panels::lyrics(self, ctx, m, false);
                            theme::apply(&mut lyrics, self.dark);
                            self.buttons.apply(
                                &mut lyrics,
                                ctx.input(|i| i.pointer.hover_pos()),
                                ctx.input(|i| i.pointer.primary_down()),
                                now,
                                self.reduced_motion,
                                self.dark,
                            );
                            self.last_lyrics = Some(lyrics);
                        }
                        if opacity > 0.001 {
                            if let Some(mut lyrics) = self.last_lyrics.clone() {
                                lyrics.opacity = opacity;
                                self.renderer.paint(
                                    ui.painter(),
                                    &lyrics,
                                    transform,
                                    1.,
                                    true,
                                    m.settings.lyrics_enabled,
                                );
                            }
                        }
                    } else {
                        self.lyrics_visibility = transitions::Tween::new(0.);
                    }
                    if !self.zen {
                        if let Some(dock) = root.find("Floating player") {
                            if let Some(speaker) = dock.find("Mute") {
                                let anchor = Rect::from_min_size(
                                    pos2(dock.x + speaker.x, dock.y + speaker.y),
                                    vec2(speaker.width, speaker.height),
                                );
                                let popup = panels::volume(self, m, anchor);
                                let p = &popup.children[0];
                                let bounds =
                                    Rect::from_min_size(pos2(p.x, p.y), vec2(p.width, p.height))
                                        .union(anchor)
                                        .expand(8.);
                                if ctx.input(|i| {
                                    i.pointer.hover_pos().is_some_and(|p| {
                                        anchor.expand(4.).contains(p)
                                            || (now < self.volume_until && bounds.contains(p))
                                    })
                                }) || matches!(self.drag, Some((Control::VerticalVolume, _, _)))
                                {
                                    self.volume_until = now + 0.25;
                                }
                                let opacity = self.volume_visibility.value(
                                    if now < self.volume_until { 1. } else { 0. },
                                    now,
                                    self.reduced_motion,
                                );
                                if opacity > 0.001 {
                                    let mut popup = popup;
                                    theme::apply(&mut popup, self.dark);
                                    popup.opacity = opacity;
                                    self.renderer.paint(
                                        ui.painter(),
                                        &popup,
                                        transform,
                                        1.,
                                        true,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
                if let Some(root) = &closing {
                    self.renderer
                        .paint(ui.painter(), root, transform, 1., true, false);
                }
                if let Some(root) = &overlay {
                    self.renderer
                        .paint(ui.painter(), root, transform, 1., true, true);
                }
                self.input(ui, origin, scale, &mut out, m);
                if !m.message.is_empty() && now - self.message_at < 7. && self.overlay.is_none() {
                    let mut toast = self.root("14:2385");
                    let message = m.message.chars().take(70).collect::<String>();
                    toast.texts(&[&message]);
                    if m.message == crate::diagnostics::USER_ERROR {
                        toast.visit(&mut |n| {
                            if n.asset.as_deref() == Some("check") {
                                n.asset = Some("x".into());
                                n.tint = serde_json::json!({"r":0.80,"g":0.20,"b":0.18});
                            }
                        });
                    }
                    if let Some(card) = toast.find_mut("Toast") {
                        card.width = card
                            .width
                            .min(self.layout.size.x - self.layout.surface_x - self.layout.margin);
                        let width = card.width;
                        card.visit(&mut |n| {
                            if n.kind == "TEXT" {
                                n.width = (width - 68.).max(60.);
                                n.natural_width = 0.;
                            }
                        });
                        card.move_to(
                            self.layout.surface_x
                                + (self.layout.size.x - self.layout.surface_x - card.width) / 2.,
                            self.layout.player_y - 76.,
                        );
                    }
                    theme::apply(&mut toast, self.dark);
                    self.renderer
                        .paint(ui.painter(), &toast, transform, 1., true, false);
                }
                self.renderer.finish(ctx);
            });
        if self.page == Page::Artist && self.overlay.is_none() && !self.zen {
            if let Some(catalog) = &m.artist {
                if !catalog.loading && !catalog.failed && catalog.next_page.is_some() {
                    let offset = self.offsets.get("artist").copied().unwrap_or(0.);
                    if self
                        .renderer
                        .regions
                        .get("list/artist")
                        .is_some_and(|(bounds, clip)| {
                            offset + bounds.intersect(*clip).height()
                                >= catalog.tracks.len() as f32 * 68. - 136.
                        })
                    {
                        out.push(Intent::ArtistMore);
                    }
                }
            }
        }
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        if crate::platform::mobile() && self.overlay.is_none() {
            if let (Some((start, at)), Some(current)) = (self.touch_start, pointer) {
                if !self.touch_long
                    && self.drag.is_none()
                    && ctx.input(|i| i.pointer.primary_down() && i.time - at > 0.55)
                    && current.distance(start) < 8.
                {
                    let track = self
                        .renderer
                        .hits
                        .iter()
                        .rev()
                        .find(|h| h.contains(current))
                        .and_then(|hit| match &hit.control {
                            Control::Play(track) | Control::Row(track) => Some(track.clone()),
                            Control::QueueIndex(index) => m.queue.get(*index).cloned(),
                            _ => None,
                        });
                    if let Some(track) = track {
                        self.touch_long = true;
                        self.process(
                            Control::Row(track),
                            m,
                            &mut out,
                            egui::Pos2::ZERO,
                            egui::Vec2::ZERO,
                        );
                    }
                }
                if ctx.input(|i| i.pointer.primary_released())
                    && self.page == Page::Sonora
                    && !self.zen
                {
                    let dx = current.x - start.x;
                    let dy = current.y - start.y;
                    let over_cover = self
                        .renderer
                        .regions
                        .get("Cover flow · swipe")
                        .is_some_and(|(bounds, clip)| bounds.intersect(*clip).contains(start));
                    if over_cover && dx.abs() > 45. && dx.abs() > dy.abs() * 1.3 {
                        out.push(if dx < 0. {
                            Intent::Next
                        } else {
                            Intent::Previous
                        });
                        self.touch_long = true;
                    }
                }
            }
        }
        if let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) {
            let wheel = ctx.input(|i| i.smooth_scroll_delta.y) + self.touch_delta;
            let (name, key, max) = if self.overlay.is_some() {
                (
                    "Dialog content",
                    "dialog-scroll".to_owned(),
                    self.dialog_scroll_max,
                )
            } else if m.settings.lyrics_enabled
                && self
                    .renderer
                    .regions
                    .get("Lyrics content")
                    .is_some_and(|(bounds, clip)| bounds.intersect(*clip).contains(pointer))
            {
                (
                    "Lyrics content",
                    "lyrics-scroll".to_owned(),
                    self.lyrics_scroll_max,
                )
            } else {
                (
                    "Main content",
                    format!("page/{:?}", self.page),
                    self.page_scroll_max,
                )
            };
            let over_track_list = self.overlay.is_none()
                && ["results", "artist", "favorites", "playlist"]
                    .iter()
                    .any(|key| {
                        self.renderer
                            .regions
                            .get(&format!("list/{key}"))
                            .is_some_and(|(bounds, clip)| bounds.intersect(*clip).contains(pointer))
                    });
            if max > 0.
                && !(name == "Main content" && over_track_list)
                && self
                    .renderer
                    .regions
                    .get(name)
                    .is_some_and(|(bounds, clip)| bounds.intersect(*clip).contains(pointer))
            {
                let value = self.offsets.entry(key).or_default();
                *value = (*value - wheel).clamp(0., max);
            }
        }
        if let Some(pointer) = pointer {
            let hit = self
                .renderer
                .hits
                .iter()
                .rev()
                .find(|h| h.contains(pointer));
            let credential_field = self.overlay == Some(Overlay::Credentials)
                && (0..3).any(|index| {
                    self.renderer
                        .regions
                        .get(&format!("Credential input/{index}"))
                        .is_some_and(|(bounds, clip)| bounds.intersect(*clip).contains(pointer))
                });
            if credential_field {
                ctx.set_cursor_icon(egui::CursorIcon::Text);
            } else if hit.is_some_and(|hit| !matches!(hit.control, Control::Noop)) {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if ctx.input(|i| i.pointer.primary_pressed()) {
                if let Some(hit) = hit {
                    if matches!(
                        hit.control,
                        Control::Seek
                            | Control::Volume
                            | Control::VerticalVolume
                            | Control::EqBand(_)
                            | Control::Preamp
                            | Control::Crossfade
                    ) {
                        self.drag = Some((hit.control.clone(), hit.transform, hit.size));
                    }
                }
            }
            if let Some((control, t, size)) = self.drag.clone() {
                if ctx.input(|i| i.pointer.primary_down() || i.pointer.primary_released()) {
                    if matches!(control, Control::Seek) {
                        let duration = if m.status.duration > 0. {
                            m.status.duration
                        } else {
                            self.current(m).map(|t| t.duration as f32).unwrap_or(0.)
                        };
                        self.seek_preview =
                            Some((t.inverse(pointer).x / size.x).clamp(0., 1.) * duration);
                        if ctx.input(|i| i.pointer.primary_released()) {
                            self.process(control, m, &mut out, t.inverse(pointer), size);
                            self.seek_preview = None;
                        }
                    } else {
                        self.process(control, m, &mut out, t.inverse(pointer), size);
                    }
                }
            } else if !self.touch_long && ctx.input(|i| i.pointer.primary_clicked()) {
                if let Some(hit) = hit {
                    let control = hit.control.clone();
                    let local = hit.transform.inverse(pointer);
                    let size = hit.size;
                    self.process(control, m, &mut out, local, size);
                }
            }
            if ctx.input(|i| i.pointer.secondary_clicked()) {
                if let Some(hit) = self
                    .renderer
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.contains(pointer))
                {
                    let track = match &hit.control {
                        Control::Play(t) | Control::Like(t) | Control::Row(t) => Some(t.clone()),
                        Control::QueueIndex(i) => m.queue.get(*i).cloned(),
                        _ => None,
                    };
                    if let Some(t) = track {
                        self.process(
                            Control::Row(t),
                            m,
                            &mut out,
                            egui::Pos2::ZERO,
                            egui::Vec2::ZERO,
                        );
                    }
                }
            }
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.drag = None;
            self.seek_preview = None;
            self.touch_start = None;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(if self.reduced_motion {
            100
        } else {
            16
        }));
        out
    }
}

/// Real recordings used by the Figma prototype and as optional discovery seeds.
/// They are resolved through providers before playback, never treated as streams.
pub fn demo_tracks() -> Vec<Track> {
    [
        ("charlie", "How Long", "Charlie Puth", 200),
        ("weeknd", "Blinding Lights", "The Weeknd", 200),
        ("rihanna", "Needed Me", "Rihanna", 191),
        ("massive", "Teardrop", "Massive Attack", 330),
        ("arctic", "Do I Wanna Know?", "Arctic Monkeys", 272),
        ("oneheart", "snowfall", "Øneheart · reidenshi", 124),
    ]
    .into_iter()
    .map(|(key, title, artist, duration)| Track {
        album: match key {
            "charlie" => "Voicenotes",
            "weeknd" => "After Hours",
            "rihanna" => "ANTI",
            "massive" => "Mezzanine",
            "arctic" => "AM",
            _ => "snowfall",
        }
        .into(),
        id: key.into(),
        provider: Provider::LastFm,
        title: title.into(),
        artist: artist.into(),
        duration,
        cover: Some(key.into()),
        page_url: String::new(),
        edition: Edition::Unknown,
        reason: String::new(),
        score: 0.,
    })
    .collect()
}

fn reduced_motion() -> bool {
    if std::env::var_os("WEDEX_REDUCED_MOTION").is_some() {
        return true;
    }
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SystemParametersInfoW(
                action: u32,
                param: u32,
                value: *mut std::ffi::c_void,
                flags: u32,
            ) -> i32;
        }
        let mut enabled = 1_i32;
        // SPI_GETCLIENTAREAANIMATION is a read of the Windows animation preference.
        if unsafe { SystemParametersInfoW(0x1042, 0, (&mut enabled as *mut i32).cast(), 0) } != 0 {
            return enabled == 0;
        }
    }
    false
}

#[cfg(test)]
mod tests;
