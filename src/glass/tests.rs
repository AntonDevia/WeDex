use super::*;
use egui::{Event, Modifiers, PointerButton, RawInput};

#[test]
fn synchronized_connections_preserve_the_open_player_panel() {
    let mut h = Harness::new();
    h.ui.overlay = Some(Overlay::Eq);
    h.ui.connection_updated(Provider::LastFm, "synthetic-synced-key");
    assert_eq!(h.ui.overlay, Some(Overlay::Eq));
    assert_eq!(h.ui.lastfm, "synthetic-synced-key");
    h.ui.connection_updated(Provider::LastFm, "");
    assert_eq!(h.ui.overlay, Some(Overlay::Eq));
    assert!(h.ui.lastfm.is_empty());
}

#[test]
fn updates_open_at_the_top_after_scrolling_settings() {
    let _mobile = crate::platform::mobile_for_test();
    let m = fixture();
    let mut h = Harness::new();
    h.size = vec2(411., 838.);
    h.ui.overlay = Some(Overlay::Settings);
    h.frame(&m, vec![]);
    h.ui.offsets.insert("dialog-scroll".into(), 300.);
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Overlay(Overlay::Updates)), 0.5);
    h.click(&m, p);
    h.frame(&m, vec![]);
    assert_eq!(h.ui.overlay, Some(Overlay::Updates));
    assert_eq!(h.ui.offsets.get("dialog-scroll").copied().unwrap_or(0.), 0.);
    let body =
        h.ui.last_overlay
            .as_ref()
            .unwrap()
            .find("Panel body")
            .unwrap();
    assert_eq!(body.y, 0.);
}

#[cfg(windows)]
#[test]
fn vps_form_keeps_focus_and_clears_secrets_on_submit_or_close() {
    let m = fixture();
    let mut h = Harness::new();
    h.ui.overlay = Some(Overlay::Vps);
    h.frame(&m, vec![]);
    let field = h.ui.renderer.regions["VPS input/0"].0.center();
    h.click(&m, field);
    assert_eq!(h.ui.overlay, Some(Overlay::Vps));
    h.ui.vps_editor.fields[0] = "203.0.113.25".into();
    h.ui.vps_editor.fields[3] = "test-secret".into();
    let request = h.ui.vps_editor.request(false, "My PC").unwrap();
    assert_eq!(request.password.as_str(), "test-secret");
    assert!(h.ui.vps_editor.fields[3].is_empty());
    h.ui.vps_editor.fields[8] = "test-sudo".into();
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Close), 0.5);
    h.click(&m, p);
    assert!(h.ui.vps_editor.fields[8].is_empty());
    assert!(h.ui.overlay.is_none());
}

#[test]
fn phone_device_picker_is_in_player_and_cloud_settings_only_connect() {
    let _mobile = crate::platform::mobile_for_test();
    let m = fixture();
    let mut h = Harness::new();
    h.size = vec2(411., 911.);
    h.ui.zen = true;
    h.ui.cloud_view = crate::cloud::View {
        connected: true,
        own_device: "phone".into(),
        selected: "pc".into(),
        devices: vec![wedex_cloud::Device {
            id: "pc".into(),
            name: "My PC".into(),
            can_play: true,
            online: true,
            updated_ms: 1,
            playback: Default::default(),
        }],
        ..Default::default()
    };
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Overlay(Overlay::Devices)), 0.5);
    h.click(&m, p);
    assert_eq!(h.ui.overlay, Some(Overlay::Devices));
    h.frame(&m, vec![]);
    let p = h.point(
        |c| matches!(c,Control::Cloud(crate::cloud::Action::Target(id)) if id=="pc"),
        0.5,
    );
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i,Intent::Cloud(crate::cloud::Action::Target(id)) if id=="pc")));
    assert!(h.ui.overlay.is_none());
    h.ui.overlay = Some(Overlay::Cloud);
    h.frame(&m, vec![]);
    assert!(h.ui.renderer.hits.iter().all(|hit| !matches!(
        &hit.control,
        Control::Cloud(crate::cloud::Action::Target(_) | crate::cloud::Action::Remote(_))
    )));
}

#[test]
fn mobile_dock_has_only_like_and_play_and_opens_zen_from_the_track() {
    let _mobile = crate::platform::mobile_for_test();
    let m = fixture();
    let mut h = Harness::new();
    h.size = vec2(411., 838.);
    h.frame(&m, vec![]);
    let mut dock =
        h.ui.last_root
            .as_ref()
            .unwrap()
            .find("Floating player")
            .unwrap()
            .clone();
    assert_eq!(dock.height, 64.);
    assert_eq!(dock.children.len(), 4);
    dock.visit(&mut |n| {
        if let Some(control) = &n.command {
            assert!(matches!(
                control,
                Control::Zen(true) | Control::Like(_) | Control::PlayPause | Control::Noop
            ));
        }
    });
    let like = h.point(|c| matches!(c, Control::Like(_)), 0.5);
    // Select the like belonging to the dock, since the Sonora page also has tracks.
    let like = if like.y >= dock.y {
        like
    } else {
        pos2(dock.x + dock.width - 86., dock.y + 32.)
    };
    assert!(h
        .click(&m, like)
        .iter()
        .any(|i| matches!(i, Intent::Like(_))));
    let play = h.point(|c| matches!(c, Control::PlayPause), 0.5);
    assert!(h.click(&m, play).iter().any(|i| matches!(i, Intent::Pause)));
    assert!(h
        .click(&m, pos2(dock.x + 110., dock.y + 32.))
        .iter()
        .any(|i| matches!(i, Intent::Zen(true))));
}

#[test]
fn mobile_zen_keeps_artwork_clean_and_places_working_controls_below_it() {
    let _mobile = crate::platform::mobile_for_test();
    let mut m = fixture();
    let mut h = Harness::new();
    h.ui.zen = true;
    for size in [
        vec2(320., 480.),
        vec2(360., 640.),
        vec2(411., 911.),
        vec2(800., 1280.),
        vec2(838., 411.),
    ] {
        h.size = size;
        h.frame(&m, vec![]);
        let root = h.ui.last_root.as_ref().unwrap();
        let art = root.find("Zen artwork").unwrap();
        assert!(art.children.is_empty());
        assert!(art.command.is_none());
        let meta = root.find("Zen mobile/metadata").unwrap();
        let seek = root.find("Zen seek").unwrap();
        let controls = root.find("Zen mobile/transport").unwrap();
        let secondary = root.find("Zen mobile/secondary").unwrap();
        if size.x < size.y {
            assert!(meta.y >= art.y + art.height);
        }
        assert!(seek.y >= meta.y + meta.height);
        assert!(controls.y >= seek.y + seek.height);
        assert!(secondary.y >= controls.y + controls.height);
        for n in [art, meta, seek, controls, secondary] {
            assert!(n.x >= 0. && n.x + n.width <= size.x);
            assert!(n.y >= 0. && n.y + n.height <= size.y);
        }
    }
    h.size = vec2(411., 911.);
    h.frame(&m, vec![]);
    type Check = (fn(&Control) -> bool, fn(&Intent) -> bool);
    let checks: [Check; 3] = [
        (
            |c: &Control| matches!(c, Control::Next),
            |i: &Intent| matches!(i, Intent::Next),
        ),
        (
            |c: &Control| matches!(c, Control::Previous),
            |i: &Intent| matches!(i, Intent::Previous),
        ),
        (
            |c: &Control| matches!(c, Control::Radio(_)),
            |i: &Intent| matches!(i, Intent::Radio(_)),
        ),
    ];
    for (matches_control, matches_intent) in checks {
        let p = h.point(matches_control, 0.5);
        assert!(h.click(&m, p).iter().any(matches_intent));
    }
    let p = h.point(|c| matches!(c, Control::ToggleLyrics), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i, Intent::Lyrics(true))));
    m.settings.lyrics_enabled = true;
    m.lyrics = Some(crate::lyrics::Lyrics {
        lines: crate::lyrics::parse_lrc("[00:00.00]A first test line\n[00:45.00]Another test line"),
        ..Default::default()
    });
    h.frame(&m, vec![]);
    let root = h.ui.last_root.as_ref().unwrap();
    let lyrics = root.find("Zen lyrics").unwrap();
    let bounds = super::mobile_player::Geometry::new(h.size).art;
    assert_eq!(pos2(lyrics.x, lyrics.y), bounds.min);
    assert_eq!(vec2(lyrics.width, lyrics.height), bounds.size());
    assert!(root.find("Zen mobile/transport").is_some());
    assert!(root.find("Zen artwork").is_none());
}

#[test]
fn cloud_panel_fields_keep_focus_and_start_button_uses_selected_network_mode() {
    let model = fixture();
    let mut h = Harness::new();
    h.ui.overlay = Some(Overlay::Cloud);
    h.frame(&model, vec![]);
    let start = h.point(
        |c| {
            matches!(
                c,
                Control::Cloud(crate::cloud::Action::Start { lan: false })
            )
        },
        0.5,
    );
    let actions = h.click(&model, start);
    assert!(actions
        .iter()
        .any(|a| matches!(a, Intent::Cloud(crate::cloud::Action::Start { lan: false }))));
    h.frame(&model, vec![]);
    let input = h.ui.renderer.regions["Cloud input/1"].0.center();
    h.click(&model, input);
    h.frame(&model, vec![Event::Text("wedex-cloud:fixture".into())]);
    assert_eq!(h.ui.overlay, Some(Overlay::Cloud));
    assert_eq!(h.ui.cloud_editor.invitation, "wedex-cloud:fixture");
    let lan = h.point(|c| matches!(c, Control::CloudLan), 0.5);
    h.click(&model, lan);
    h.frame(&model, vec![]);
    assert!(h.ui.cloud_editor.lan);
}

fn fixture() -> Model {
    let tracks = demo_tracks();
    Model {
        player_liked: None,
        updates: Default::default(),
        sonora_ready: true,
        sonora_loading: false,
        artists: Vec::new(),
        artist: None,
        error_generation: 0,
        settings: Settings {
            volume: 0.75,
            ..Default::default()
        },
        status: Status {
            track: Some(tracks[0].clone()),
            playing: true,
            duration: 200.,
            position: 48.,
            ..Default::default()
        },
        queue: tracks.clone(),
        cursor: Some(0),
        favorites: tracks.clone(),
        results: vec![],
        playlists: vec![Playlist {
            uid: "fixture".into(),
            kind: "1".into(),
            title: "Ночной маршрут".into(),
            count: 6,
            cover: None,
        }],
        playlist_tracks: tracks,
        playlist_title: "Ночной маршрут".into(),
        query: String::new(),
        searching: false,
        loading_library: false,
        loading_playlist: false,
        resolving: false,
        recording: false,
        account: String::new(),
        connected: [true, true, false, true, true],
        lyrics: None,
        lyrics_loading: false,
        alternatives: None,
        versions_loading: false,
        message: String::new(),
        preview: true,
    }
}
#[test]
fn zen_and_lyrics_controls_work_and_empty_panels_do_not_close_on_click() {
    let mut m = fixture();
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    let artwork = h.point(|c| matches!(c, Control::Zen(true)), 0.5);
    assert!(h
        .click(&m, artwork)
        .iter()
        .any(|i| matches!(i, Intent::Zen(true))));
    h.frame(&m, vec![]);
    let cover = h.ui.renderer.regions["Zen artwork"].0;
    h.frame(&m, vec![Event::PointerMoved(cover.center())]);
    let lyrics = h.point(|c| matches!(c, Control::ToggleLyrics), 0.5);
    assert!(h
        .click(&m, lyrics)
        .iter()
        .any(|i| matches!(i, Intent::Lyrics(true))));
    m.settings.lyrics_enabled = true;
    m.lyrics = Some(crate::lyrics::Lyrics {
        lines: crate::lyrics::parse_lrc(
            "[00:00.00]First fixture\n[00:24.00]Second fixture\n[00:45.00]Third fixture",
        ),
        ..Default::default()
    });
    h.frame(&m, vec![]);
    let line = h.point(|c| matches!(c,Control::LyricSeek(t)if *t==45.), 0.5);
    assert!(h
        .click(&m, line)
        .iter()
        .any(|i| matches!(i,Intent::Seek(t)if *t==45.)));
    let exit = h.point(|c| matches!(c, Control::Zen(false)), 0.5);
    assert!(h
        .click(&m, exit)
        .iter()
        .any(|i| matches!(i, Intent::Zen(false))));
    h.ui.overlay = Some(Overlay::Settings);
    h.frame(&m, vec![]);
    h.click(&m, pos2(800., 370.));
    assert_eq!(h.ui.overlay, Some(Overlay::Settings));
}
#[test]
fn every_tab_shares_ambient_and_fontawesome_masks_support_both_themes() {
    let m = fixture();
    let mut h = Harness::new();
    for page in [
        Page::Sonora,
        Page::Search,
        Page::Library,
        Page::Favorites,
        Page::Playlists,
        Page::Playlist,
    ] {
        h.ui.page = page;
        h.frame(&m, vec![]);
        assert!(h
            .ui
            .last_root
            .as_ref()
            .unwrap()
            .find("Ambient light")
            .is_some());
    }
    for name in [
        "play",
        "pause",
        "lyrics",
        "settings-2",
        "expand",
        "repeat",
        "list-music",
    ] {
        let image = image::load_from_memory(assets::bytes(name).unwrap())
            .unwrap()
            .into_rgba8();
        assert!(
            image
                .pixels()
                .filter(|p| p[3] > 200)
                .all(|p| p[0] > 245 && p[1] > 245 && p[2] > 245),
            "Icon {name} must be a tintable white mask"
        );
    }
}
#[test]
fn player_orders_controls_above_timeline_and_matches_sidebar_rounding() {
    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        let mut m = fixture();
        m.settings.theme = theme;
        let mut h = Harness::new();
        h.frame(&m, vec![]);
        let root = h.ui.last_root.as_ref().unwrap();
        let dock = root.find("Floating player").unwrap();
        assert_eq!(
            dock.corner_radius,
            root.find("Sidebar").unwrap().corner_radius
        );
        let names = [
            "Save current track",
            "button/skip-back",
            "Play / pause",
            "button/skip-forward",
            "Shuffle",
            "Repeat",
        ];
        let controls = names.map(|name| dock.find(name).unwrap());
        assert!(controls
            .windows(2)
            .all(|pair| pair[0].x + pair[0].width <= pair[1].x));
        assert!(controls
            .iter()
            .all(|control| control.y + control.height < dock.find("Timeline").unwrap().y));
        assert!(controls[0].width > controls[1].width);
        let glyph = controls[0].find("Active like").unwrap();
        assert_eq!(glyph.tint["g"], 1.);
        assert_eq!(glyph.tint["r"], 0.91);
    }
}
#[test]
fn zen_hover_and_lyric_tracking_animate_for_two_hundred_milliseconds() {
    fn line_scale(n: &Node, time: f32) -> Option<f32> {
        if matches!(n.command,Some(Control::LyricSeek(t))if t==time) {
            return Some(n.relative_transform.unwrap()[0][0]);
        }
        n.children.iter().find_map(|child| line_scale(child, time))
    }
    let mut m = fixture();
    m.settings.lyrics_enabled = true;
    m.status.position = 1.;
    m.lyrics=Some(crate::lyrics::Lyrics{lines:crate::lyrics::parse_lrc("[00:00.00]First fixture\n[00:12.00]Second fixture\n[00:24.00]Third fixture\n[00:36.00]Fourth fixture\n[00:48.00]Fifth fixture"),..Default::default()});
    let mut h = Harness::new();
    h.ui.reduced_motion = false;
    h.ui.zen = true;
    h.size = vec2(1920., 1080.);
    h.frame(&m, vec![]);
    let initial = h.ui.lyrics_motion.at(h.time);
    assert_eq!(line_scale(h.ui.last_root.as_ref().unwrap(), 48.), Some(0.5));
    m.status.position = 48.;
    h.frame(&m, vec![]);
    assert!((h.ui.lyrics_motion.at(h.time) - initial).abs() < 0.01);
    h.frame(&m, vec![]);
    let midway = h.ui.lyrics_motion.at(h.time);
    assert!((line_scale(h.ui.last_root.as_ref().unwrap(), 48.).unwrap() - 0.75).abs() < 0.01);
    h.frame(&m, vec![]);
    let finished = h.ui.lyrics_motion.at(h.time);
    assert_eq!(line_scale(h.ui.last_root.as_ref().unwrap(), 48.), Some(1.));
    assert!(midway > initial && midway < finished);
    assert!((midway - (initial + finished) / 2.).abs() < 0.1);
    let root = h.ui.last_root.as_ref().unwrap();
    assert!(root.find("Zen lyrics").is_some());
    assert!(root.find("Glass dialog").is_none());
    let cover = h.ui.renderer.regions["Zen artwork"].0;
    h.frame(&m, vec![Event::PointerMoved(cover.center())]);
    assert_eq!(h.ui.zen_controls.at(h.time), 0.);
    h.frame(&m, vec![]);
    assert!((h.ui.zen_controls.at(h.time) - 0.5).abs() < 0.01);
    h.frame(&m, vec![]);
    assert!((h.ui.zen_controls.at(h.time) - 1.).abs() < 0.01);
    h.frame(&m, vec![Event::PointerMoved(pos2(2., 2.))]);
    h.frame(&m, vec![]);
    assert!(h.ui.zen_controls.at(h.time) > 0. && h.ui.zen_controls.at(h.time) < 1.);
    assert!(!h
        .ui
        .renderer
        .hits
        .iter()
        .any(|hit| matches!(hit.control, Control::Next)));
}

#[test]
fn wheel_browses_synced_lyrics_uniformly_then_resumes_the_current_line_after_three_seconds() {
    for zen in [false, true] {
        let mut m = fixture();
        m.settings.lyrics_enabled = true;
        m.status.position = 100.;
        m.lyrics = Some(crate::lyrics::Lyrics {
            lines: (0..60)
                .map(|i| crate::lyrics::Line {
                    time: i as f32 * 5.,
                    text: format!("Fixture line {i}"),
                })
                .collect(),
            ..Default::default()
        });
        let mut h = Harness::new();
        h.ui.reduced_motion = false;
        h.ui.zen = zen;
        h.frame(&m, vec![]);
        h.frame(
            &m,
            vec![
                Event::PointerMoved(pos2(20., 20.)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0., -240.),
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(h.ui.lyrics_manual_until < h.time);
        let (bounds, clip) = h.ui.renderer.regions["Lyrics content"];
        h.frame(
            &m,
            vec![
                Event::PointerMoved(bounds.intersect(clip).center()),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0., -240.),
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        let until = h.ui.lyrics_manual_until;
        assert_eq!(h.ui.lyrics_manual_index, Some(26));
        let root = if zen {
            h.ui.last_root.as_ref()
        } else {
            h.ui.last_lyrics.as_ref()
        }
        .unwrap();
        let content = root.find("Lyrics content").unwrap();
        let selected = content
            .children
            .iter()
            .find(|n| matches!(n.command, Some(Control::LyricSeek(t)) if t == 130.))
            .unwrap();
        assert!(
            (selected.y + selected.height * 0.25 - content.height * 0.43).abs() < 0.01,
            "the complete row snaps into place in the input frame"
        );
        let immediate = h.ui.lyrics_motion.at(h.time);
        assert!((until - h.time - 3.).abs() < 0.001);
        for _ in 0..10 {
            h.frame(&m, vec![]);
        }
        let root = if zen {
            h.ui.last_root.as_ref()
        } else {
            h.ui.last_lyrics.as_ref()
        }
        .unwrap();
        let lines = &root.find("Lyrics content").unwrap().children;
        assert!(!lines.is_empty());
        assert!(lines.iter().all(
            |n| (n.relative_transform.unwrap()[0][0] - 0.5).abs() < 0.001
                && (n.opacity - 1.).abs() < 0.001
        ));
        let held = h.ui.lyrics_motion.at(h.time);
        assert!(
            (held - immediate).abs() < 0.01,
            "no smooth tail after releasing the wheel"
        );
        m.status.position = 150.;
        for _ in 0..5 {
            h.frame(&m, vec![]);
        }
        assert!((h.ui.lyrics_motion.at(h.time) - held).abs() < 0.1);
        h.time = until - 0.15;
        h.frame(&m, vec![]);
        assert!(h.time < until);
        assert!((h.ui.lyrics_motion.at(h.time) - held).abs() < 0.1);
        h.time = until;
        for _ in 0..4 {
            h.frame(&m, vec![]);
        }
        let root = if zen {
            h.ui.last_root.as_ref()
        } else {
            h.ui.last_lyrics.as_ref()
        }
        .unwrap();
        let active = root
            .find("Lyrics content")
            .unwrap()
            .children
            .iter()
            .find(|n| matches!(n.command, Some(Control::LyricSeek(t)) if t == 150.))
            .expect("current line returns into view");
        assert!((active.relative_transform.unwrap()[0][0] - 1.).abs() < 0.001);
        assert!((h.ui.lyrics_motion.at(h.time) - held).abs() > 100.);
        let (bounds, clip) = h.ui.renderer.regions["Lyrics content"];
        h.frame(
            &m,
            vec![
                Event::PointerMoved(bounds.intersect(clip).center()),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: vec2(0., 3.),
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(h.ui.lyrics_manual_until > h.time);
        m.status.track.as_mut().unwrap().id = "Next fixture".into();
        h.frame(&m, vec![]);
        assert!(h.ui.lyrics_manual_until < h.time);
    }
}

#[test]
fn lyric_row_scroll_handles_wrapped_lines_precision_wheels_and_both_boundaries() {
    for zen in [false, true] {
        let mut m = fixture();
        m.settings.lyrics_enabled = true;
        m.status.position = 0.;
        m.lyrics = Some(crate::lyrics::Lyrics {
            lines: (0..12)
                .map(|i| crate::lyrics::Line {
                    time: i as f32 * 5.,
                    text: if i % 2 == 0 {
                        format!("Line {i}")
                    } else {
                        "A wrapped fixture line with several words ".repeat(3)
                    },
                })
                .collect(),
            ..Default::default()
        });
        let mut h = Harness::new();
        h.ui.zen = zen;
        h.ui.reduced_motion = false;
        h.frame(&m, vec![]);
        h.frame(&m, vec![]);
        let (bounds, clip) = h.ui.renderer.regions["Lyrics content"];
        let pointer = bounds.intersect(clip).center();
        let wheel = |unit, delta| {
            vec![
                Event::PointerMoved(pointer),
                Event::MouseWheel {
                    unit,
                    delta: vec2(0., delta),
                    modifiers: Modifiers::NONE,
                },
            ]
        };
        h.frame(&m, wheel(egui::MouseWheelUnit::Line, 100.));
        assert_eq!(h.ui.lyrics_manual_index, Some(0));
        for index in 0..4 {
            h.frame(&m, wheel(egui::MouseWheelUnit::Point, -10.));
            assert_eq!(
                h.ui.lyrics_manual_index,
                Some(if index == 3 { 1 } else { 0 })
            );
        }
        let root = if zen {
            h.ui.last_root.as_ref()
        } else {
            h.ui.last_lyrics.as_ref()
        }
        .unwrap();
        let content = root.find("Lyrics content").unwrap();
        let wrapped = content
            .children
            .iter()
            .find(|n| matches!(n.command, Some(Control::LyricSeek(t)) if t == 5.))
            .unwrap();
        assert!(wrapped.height > wrapped.line_height["value"].as_f64().unwrap() as f32);
        assert!((wrapped.y + wrapped.height * 0.25 - content.height * 0.43).abs() < 0.01);
        h.frame(&m, wheel(egui::MouseWheelUnit::Line, -1.));
        assert_eq!(h.ui.lyrics_manual_index, Some(2));
        h.frame(&m, wheel(egui::MouseWheelUnit::Line, -100.));
        assert_eq!(h.ui.lyrics_manual_index, Some(11));
        h.frame(&m, wheel(egui::MouseWheelUnit::Line, 1.));
        assert_eq!(h.ui.lyrics_manual_index, Some(10));
        assert_eq!(m.status.position, 0.);
    }
}
struct Harness {
    density: f32,
    system_theme: Option<egui::Theme>,
    ctx: egui::Context,
    ui: GlassUi,
    time: f64,
    size: egui::Vec2,
}
#[test]
fn service_editors_keep_their_own_field_open_and_check_after_paste_without_saving() {
    let m = fixture();
    for size in [vec2(1440., 960.), vec2(480., 800.)] {
        for provider in [
            Provider::Yandex,
            Provider::LastFm,
            Provider::SoundCloud,
            Provider::Hitmotop,
        ] {
            let mut h = Harness::new();
            h.size = size;
            h.ui.overlay = Some(Overlay::Sources);
            h.frame(&m, vec![]);
            let p = h.point(
                |c| matches!(c, Control::Source(s) if connections::provider(s) == provider),
                0.5,
            );
            h.click(&m, p);
            h.frame(&m, vec![]);
            assert_eq!(h.ui.overlay, Some(Overlay::Credentials));
            assert_eq!(h.ui.connection.provider, provider);
            assert!(!h.ui.renderer.regions.contains_key("Credential input/1"));
            let (rect, clip) = h.ui.renderer.regions["Credential input/0"];
            h.click(&m, rect.intersect(clip).center());
            assert!(h
                .ctx
                .memory(|memory| memory.has_focus(egui::Id::new(("credential", provider)))));
            let value = if provider == Provider::Hitmotop {
                "mirror.example".into()
            } else {
                format!("invented-test-key-{provider:?}")
            };
            h.frame(
                &m,
                vec![Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers {
                        ctrl: true,
                        command: true,
                        ..Modifiers::NONE
                    },
                }],
            );
            let actions = h.frame(&m, vec![Event::Paste(value.clone())]);
            assert_eq!(h.ui.connection.draft, value);
            assert_eq!(h.ui.overlay, Some(Overlay::Credentials));
            assert!(!actions
                .iter()
                .any(|a| matches!(a, Intent::SaveConnection(..) | Intent::CheckConnection(..))));
            for _ in 0..4 {
                let actions = h.frame(&m, vec![]);
                assert!(!actions
                    .iter()
                    .any(|a| matches!(a, Intent::CheckConnection(..))));
            }
            h.time += 0.3;
            let actions = h.frame(&m, vec![]);
            let (checked_provider, revision) = actions
                .iter()
                .find_map(|i| match i {
                    Intent::CheckConnection(p, checked, r) if checked == &value => Some((*p, *r)),
                    _ => None,
                })
                .expect("automatic validation after paste");
            assert_eq!(checked_provider, provider);
            assert!(!h.ui.connection_result(
                provider,
                revision.wrapping_sub(1),
                Ok("stale".into())
            ));
            assert!(h
                .ui
                .connection_result(provider, revision, Ok("Available fixture".into())));
            assert!(!h
                .frame(&m, vec![])
                .iter()
                .any(|a| matches!(a, Intent::CheckConnection(..))));
            let p = h.point(|c| matches!(c, Control::SaveConnection), 0.5);
            let actions = h.click(&m, p);
            assert!(actions.iter().any(|a| matches!(a, Intent::SaveConnection(p, draft) if *p == provider && draft == &value)));
            assert!(h.ui.yandex.is_empty() && h.ui.lastfm.is_empty() && h.ui.soundcloud.is_empty());
            h.click(&m, pos2(2., 2.));
            assert_eq!(h.ui.overlay, None);
        }
    }
}
impl Harness {
    fn point(&self, control: impl Fn(&Control) -> bool, fraction: f32) -> egui::Pos2 {
        let hit = self
            .ui
            .renderer
            .hits
            .iter()
            .rev()
            .find(|h| control(&h.control))
            .expect("visible control");
        hit.transform.point(hit.size.x * fraction, hit.size.y / 2.)
    }
    fn new() -> Self {
        let ctx = egui::Context::default();
        crate::branding::configure(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut ui = GlassUi::new();
        ui.reduced_motion = true;
        Self {
            density: 1.,
            system_theme: Some(egui::Theme::Light),
            ctx,
            ui,
            time: 0.,
            size: vec2(1440., 960.),
        }
    }
    fn frame(&mut self, m: &Model, events: Vec<Event>) -> Vec<Intent> {
        self.time += 0.1;
        let mut actions = vec![];
        let mut input = RawInput {
            system_theme: self.system_theme,
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(self.density);
        let _ = self.ctx.run(input, |ctx| actions = self.ui.show(ctx, m));
        actions
    }
    fn click(&mut self, m: &Model, p: egui::Pos2) -> Vec<Intent> {
        self.frame(
            m,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        self.frame(
            m,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        )
    }
}

#[test]
fn theme_choices_emit_persistable_values_and_follow_system_changes() {
    let mut m = fixture();
    let mut h = Harness::new();
    h.ui.overlay = Some(Overlay::Settings);
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Theme(ThemePreference::Dark)), 0.5);
    let actions = h.click(&m, p);
    assert!(actions
        .iter()
        .any(|a| matches!(a, Intent::Theme(ThemePreference::Dark))));
    m.settings.theme = ThemePreference::Dark;
    h.frame(&m, vec![]);
    assert!(h.ui.dark);
    m.settings.theme = ThemePreference::Light;
    h.system_theme = Some(egui::Theme::Dark);
    h.frame(&m, vec![]);
    assert!(!h.ui.dark);
    m.settings.theme = ThemePreference::System;
    h.frame(&m, vec![]);
    assert!(h.ui.dark);
    h.system_theme = Some(egui::Theme::Light);
    h.frame(&m, vec![]);
    assert!(!h.ui.dark);
}

#[test]
fn sonora_has_no_old_slogans_and_only_connected_provider_marks() {
    let m = fixture();
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    let root = h.ui.last_root.as_ref().unwrap();
    for name in ["МУЗЫКА БЕЗ ЛИШНЕГО", "Ваша музыка. Новые маршруты."]
    {
        assert!(!root.find(name).unwrap().visible);
    }
    let marks = &root.find("Connected providers").unwrap().children;
    assert_eq!(marks.len(), 4);
    assert!(marks.iter().all(|n| n
        .asset
        .as_deref()
        .is_some_and(|name| assets::bytes(name).is_some())));
    assert!(!marks
        .iter()
        .any(|n| n.asset.as_deref() == Some("provider-soundcloud")));
}

#[test]
fn clicking_a_favorite_supplies_the_complete_ordered_collection() {
    let m = fixture();
    let mut h = Harness::new();
    h.ui.page = Page::Favorites;
    h.frame(&m, vec![]);
    let key = m.favorites[2].key();
    let p = h.point(|c| matches!(c,Control::Play(t)if t.key()==key), 0.5);
    let actions = h.click(&m, p);
    assert!(actions.iter().any(|a|matches!(a,Intent::PlayCollection(tracks,2)if tracks.iter().map(Track::key).collect::<Vec<_>>()==m.favorites.iter().map(Track::key).collect::<Vec<_>>())));
    assert!(!actions.iter().any(|a| matches!(a, Intent::Radio(_))));
}

#[test]
fn reset_button_is_actionable_in_both_wide_and_compact_sonora() {
    let m = fixture();
    let mut h = Harness::new();
    for size in [vec2(1440., 960.), vec2(480., 800.), vec2(360., 480.)] {
        h.size = size;
        h.frame(&m, vec![]);
        let p = h.point(|c| matches!(c, Control::ResetRadio), 0.5);
        assert!(Rect::from_min_size(egui::Pos2::ZERO, size).contains(p));
        let actions = h.click(&m, p);
        assert!(actions.iter().any(|a| matches!(a, Intent::ResetRadio)));
    }
}

#[test]
fn sonora_starts_with_one_actionable_card_and_waits_for_a_real_radio_batch() {
    let mut m = fixture();
    m.sonora_ready = false;
    m.queue.clear();
    m.cursor = None;
    m.status = Status::default();
    for size in [vec2(1440., 960.), vec2(480., 800.), vec2(360., 480.)] {
        let mut h = Harness::new();
        h.size = size;
        h.frame(&m, vec![]);
        let stage =
            h.ui.last_root
                .as_ref()
                .unwrap()
                .find("Cover flow · swipe")
                .unwrap();
        assert_eq!(stage.children.len(), 1);
        assert_eq!(stage.children[0].name, "Album/sonora-start");
        let (bounds, clip) = h.ui.renderer.regions["Album/sonora-start"];
        assert!(bounds.intersect(clip).contains(bounds.center()));
        assert!(h
            .click(&m, bounds.center())
            .iter()
            .any(|i| matches!(i, Intent::ResetRadio)));
        m.sonora_loading = true;
        h.frame(&m, vec![]);
        let actions = h.click(&m, bounds.center());
        assert!(!actions.iter().any(|i| matches!(i, Intent::ResetRadio)));
        m.sonora_loading = false;
        m.sonora_ready = true;
        m.queue = demo_tracks();
        h.frame(&m, vec![]);
        assert!(h
            .ui
            .last_root
            .as_ref()
            .unwrap()
            .find("Cover flow · swipe")
            .unwrap()
            .children
            .iter()
            .all(|n| n.name != "Album/sonora-start"));
        m.sonora_ready = false;
        m.queue.clear();
    }
}

#[test]
fn listen_all_replaces_the_queue_as_one_collection_action() {
    let m = fixture();
    let mut h = Harness::new();
    h.ui.page = Page::Favorites;
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Listen(false)), 0.5);
    let actions = h.click(&m, p);
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(&actions[0],Intent::PlayCollection(tracks,0)if tracks.len()==m.favorites.len())
    );
}

#[test]
fn shuffled_collection_keeps_every_track_and_can_restart_after_the_end() {
    let mut m = fixture();
    let mut h = Harness::new();
    h.ui.page = Page::Favorites;
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Listen(true)), 0.5);
    let actions = h.click(&m, p);
    let tracks = actions
        .iter()
        .find_map(|a| {
            if let Intent::PlayCollection(tracks, 0) = a {
                Some(tracks)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(tracks.len(), m.favorites.len());
    assert_eq!(
        tracks
            .iter()
            .map(Track::key)
            .collect::<std::collections::HashSet<_>>(),
        m.favorites.iter().map(Track::key).collect()
    );
    m.status.ended = true;
    m.status.playing = false;
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::PlayPause), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|a| matches!(a, Intent::QueueIndex(0))));
}

#[test]
fn upcoming_cover_starts_loading_before_its_card_becomes_visible() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let received = Arc::new(AtomicBool::new(false));
    let server_flag = received.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok((mut socket, _)) = listener.accept() {
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = [0_u8; 2048];
                let _ = socket.read(&mut request);
                let png = assets::bytes("check").unwrap();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",png.len()).unwrap();
                socket.write_all(png).unwrap();
                server_flag.store(true, Ordering::Relaxed);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let mut m = fixture();
    m.preview = false;
    m.queue[4].cover = Some(format!("http://{addr}/future.png"));
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    assert!(!h
        .ui
        .last_root
        .as_ref()
        .unwrap()
        .find("Cover flow · swipe")
        .unwrap()
        .children
        .iter()
        .any(|n| n.visible && n.name == "Album/4"));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !received.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        received.load(Ordering::Relaxed),
        "offscreen cover was not requested in advance"
    );
    server.join().unwrap();
}
#[test]
fn native_navigation_and_modal_hit_testing() {
    let m = fixture();
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    let actions = h.click(&m, pos2(115., 227.));
    assert!(actions
        .iter()
        .any(|a| matches!(a, Intent::Navigate(Page::Search))));
    assert_eq!(h.ui.page, Page::Search);
    h.frame(&m, vec![]);
    let queue = h.point(|c| matches!(c, Control::Overlay(Overlay::Queue)), 0.5);
    h.click(&m, queue);
    assert_eq!(h.ui.overlay, Some(Overlay::Queue));
    // A click over the obscured player dismisses the modal, without pausing audio.
    let play = h.ui.layout.surface_x + 368.;
    let actions = h.click(&m, pos2(play, h.ui.layout.player_y + 44.));
    assert!(!actions.iter().any(|a| matches!(a, Intent::Pause)));
    assert_eq!(h.ui.overlay, None);
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::PlayPause), 0.5);
    let actions = h.click(&m, p);
    assert!(actions.iter().any(|a| matches!(a, Intent::Pause)));
}
#[test]
fn eq_volume_and_timeline_use_real_continuous_values() {
    let m = fixture();
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    let speaker = h.point(|c| matches!(c, Control::Mute), 0.5);
    h.frame(&m, vec![Event::PointerMoved(speaker)]);
    let p = h.point(|c| matches!(c, Control::VerticalVolume), 0.5);
    let actions = h.click(&m, p);
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Volume(v)if (*v-0.5).abs()<0.02)));
    let p = h.point(|c| matches!(c, Control::Seek), 0.75);
    let actions = h.click(&m, p);
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Seek(v)if (*v-150.).abs()<1.)));
    let p = h.point(|c| matches!(c, Control::Overlay(Overlay::Eq)), 0.5);
    h.click(&m, p);
    assert_eq!(h.ui.overlay, Some(Overlay::Eq));
    h.frame(&m, vec![]);
    let band =
        h.ui.renderer
            .hits
            .iter()
            .find(|hit| matches!(hit.control, Control::EqBand(0)))
            .unwrap();
    let point = band
        .transform
        .point(band.size.x / 2., 28. + (12. - 9.) / 24. * 160.);
    let actions = h.click(&m, point);
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Eq(g)if g[0]==9.&&g[1..].iter().all(|v|*v==0.))));
    let p = h.point(|c| matches!(c, Control::Preset(4)), 0.5);
    let actions = h.click(&m, p);
    assert!(actions.iter().any(|a| matches!(a, Intent::Preset(4))));
}
#[test]
fn long_lists_scroll_without_clicking_clipped_rows() {
    let mut m = fixture();
    m.preview = false;
    m.favorites = (0..100)
        .map(|i| {
            let mut t = demo_tracks()[0].clone();
            t.id = i.to_string();
            t.title = format!("Трек {i}");
            t
        })
        .collect();
    let mut h = Harness::new();
    h.ui.page = Page::Favorites;
    h.frame(&m, vec![]);
    let before = h.ui.offsets.get("favorites").copied().unwrap_or(0.);
    h.frame(
        &m,
        vec![
            Event::PointerMoved(pos2(600., 600.)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0., -800.),
                modifiers: Modifiers::NONE,
            },
        ],
    );
    for _ in 0..10 {
        h.frame(&m, vec![]);
    }
    assert!(h.ui.offsets["favorites"] > before + 100.);
    assert!(h
        .ui
        .renderer
        .hits
        .iter()
        .all(|hit| !matches!(hit.control, Control::Play(_))
            || hit.clip.max.y <= h.ui.layout.player_y - 24.));
}
#[test]
fn search_keyboard_and_recognition_return_backend_intents() {
    let m = fixture();
    let mut h = Harness::new();
    h.ui.page = Page::Search;
    h.frame(&m, vec![]);
    let field = h.ui.renderer.regions["Search field"].0;
    h.click(&m, field.min + vec2(64., 26.));
    h.frame(&m, vec![Event::Text("Teardrop".into())]);
    let actions = h.frame(
        &m,
        vec![Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Search(query)if query=="Teardrop")));
    let p = h.point(|c| matches!(c, Control::Mic), 0.5);
    let actions = h.click(&m, p);
    assert!(actions.iter().any(|a| matches!(a, Intent::Recognize)));
    assert_eq!(h.ui.overlay, Some(Overlay::Recognition));
    h.ui.recognized = Some(demo_tracks()[3].clone());
    h.frame(&m, vec![]);
    assert_eq!(h.ui.overlay, Some(Overlay::Recognized));
    let p = h.point(|c| matches!(c, Control::RecognizedSearch), 0.5);
    let actions = h.click(&m, p);
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Search(query)if query=="Massive Attack Teardrop")));
}

fn search_fixture() -> Model {
    let mut m = fixture();
    m.query = "Artist".into();
    m.results = (0..100)
        .map(|i| {
            let mut track = demo_tracks()[0].clone();
            track.id = i.to_string();
            track.provider = if i % 2 == 0 {
                Provider::Yandex
            } else {
                Provider::Hitmotop
            };
            track
        })
        .collect();
    m.artists = vec![model::Artist {
        id: "42".into(),
        name: "Artist".into(),
        cover: None,
        tracks: Some(201),
        listeners: None,
    }];
    m
}

#[test]
fn search_list_scrolls_to_the_last_track_without_moving_the_page_at_laptop_sizes_and_scaling() {
    let m = search_fixture();
    for (size, density) in [
        (vec2(1366., 768.), 1.),
        (vec2(1093., 614.), 1.25),
        (vec2(911., 512.), 1.5),
        (vec2(800., 600.), 1.),
        (vec2(800., 480.), 1.),
        (vec2(1440., 480.), 1.5),
        (vec2(480., 800.), 2.),
        (vec2(360., 480.), 1.),
    ] {
        let mut h = Harness::new();
        h.size = size;
        h.density = density;
        h.ui.page = Page::Search;
        h.frame(&m, vec![]);
        let field = h.ui.renderer.regions["Search field"].0;
        let (list, clip) = h.ui.renderer.regions["list/results"];
        assert!(list.height() >= 68., "a whole track must fit at {size:?}");
        assert!(list.bottom() <= h.ui.layout.player_y - 15.);
        for _ in 0..16 {
            h.frame(
                &m,
                vec![
                    Event::PointerMoved(list.intersect(clip).center()),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0., -1000.),
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        for _ in 0..10 {
            h.frame(&m, vec![]);
        }
        assert!(
            h.ui.offsets["results"] > 6000.,
            "scroll stuck at {size:?}, density {density}"
        );
        assert_eq!(h.ui.page_scroll_max, 0.);
        assert_eq!(h.ui.renderer.regions["Search field"].0, field);
        assert!(h.ui.renderer.hits.iter().any(
            |hit| matches!(&hit.control, Control::Play(track) if track.id == "99")
                && hit
                    .clip
                    .intersects(hit.transform.bounds(hit.size.x, hit.size.y))
        ));
        let p = h.point(|c| matches!(c, Control::Source(s) if s == "filter/2"), 0.5);
        h.click(&m, p);
        h.frame(&m, vec![]);
        assert_eq!(h.ui.offsets["results"], 0.);
        let list =
            h.ui.last_root
                .as_ref()
                .unwrap()
                .find("list/results")
                .unwrap();
        assert!(list
            .children
            .iter()
            .filter_map(|n| n.command.as_ref())
            .all(|c| matches!(c, Control::Play(t) if t.provider == Provider::Hitmotop)));
    }
}

#[test]
fn artist_navigation_preserves_search_and_plays_the_artist_collection() {
    let mut m = search_fixture();
    let mut h = Harness::new();
    h.ui.page = Page::Search;
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Artist(_)), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i, Intent::Artist(a) if a.id == "42")));
    m.artist = Some(model::ArtistCatalog {
        artist: m.artists[0].clone(),
        tracks: m.results.clone(),
        total: Some(201),
        next_page: Some(1),
        loading: false,
        failed: false,
    });
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::ArtistTrack(0)), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i, Intent::PlayCollection(tracks, 0) if tracks.len() == 100)));
    let p = h.point(|c| matches!(c, Control::ArtistMore), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i, Intent::ArtistMore)));
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Navigate(Page::Search)), 0.5);
    h.click(&m, p);
    h.frame(&m, vec![]);
    assert_eq!(h.ui.page, Page::Search);
    assert_eq!(h.ui.query, "Artist");
    assert!(h
        .ui
        .last_root
        .as_ref()
        .unwrap()
        .find("Artist result")
        .is_some());
}

#[test]
fn all_artist_matches_are_accessible_and_small_artist_pages_keep_the_list_above_the_player() {
    let mut m = search_fixture();
    m.artists = (0..12)
        .map(|i| model::Artist {
            id: i.to_string(),
            name: format!("Artist {i}"),
            ..m.artists[0].clone()
        })
        .collect();
    let mut h = Harness::new();
    h.ui.page = Page::Search;
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::Overlay(Overlay::Artists)), 0.5);
    h.click(&m, p);
    h.frame(&m, vec![]);
    assert_eq!(h.ui.overlay, Some(Overlay::Artists));
    let (bounds, clip) = h.ui.renderer.regions["Dialog content"];
    for _ in 0..8 {
        h.frame(
            &m,
            vec![
                Event::PointerMoved(bounds.intersect(clip).center()),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0., -300.),
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    for _ in 0..10 {
        h.frame(&m, vec![]);
    }
    let p = h.point(|c| matches!(c, Control::Artist(a) if a.id == "11"), 0.5);
    assert!(h
        .click(&m, p)
        .iter()
        .any(|i| matches!(i, Intent::Artist(a) if a.id == "11")));
    m.artist = Some(model::ArtistCatalog {
        artist: m.artists[11].clone(),
        tracks: m.results.clone(),
        total: Some(201),
        next_page: Some(1),
        loading: false,
        failed: false,
    });
    for size in [
        vec2(360., 480.),
        vec2(480., 800.),
        vec2(911., 512.),
        vec2(800., 480.),
        vec2(1440., 480.),
        vec2(1280., 720.),
    ] {
        h.size = size;
        h.frame(&m, vec![]);
        let (bounds, clip) = h.ui.renderer.regions["list/artist"];
        assert!(
            bounds.height() >= 68.,
            "a whole artist track must fit at {size:?}"
        );
        assert!(bounds.intersect(clip).bottom() <= h.ui.layout.player_y - 15.);
        assert_eq!(h.ui.page_scroll_max, 0.);
    }
}
#[test]
fn version_selection_keeps_recording_identity() {
    let mut m = fixture();
    let original = m.queue[0].clone();
    let mut candidate = original.clone();
    candidate.id = "alternate".into();
    candidate.provider = Provider::Hitmotop;
    m.alternatives = Some((original.clone(), vec![candidate.clone()], vec![]));
    let mut h = Harness::new();
    h.ui.overlay = Some(Overlay::Versions);
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::VersionSelect(0)), 0.5);
    h.click(&m, p);
    assert_eq!(h.ui.selected_version, Some(0));
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::VersionApply), 0.5);
    let actions = h.click(&m, p);
    assert!(actions.iter().any(
        |a| matches!(a,Intent::SaveVersion(a,b)if a.key()==original.key()&&b.key()==candidate.key())
    ));
}
#[test]
fn responsive_windows_fill_the_viewport_and_keep_player_controls_accessible() {
    let m = fixture();
    let mut h = Harness::new();
    for size in [
        vec2(360., 480.),
        vec2(480., 800.),
        vec2(800., 600.),
        vec2(1280., 720.),
        vec2(1920., 1080.),
        vec2(2560., 1440.),
    ] {
        h.size = size;
        h.frame(&m, vec![]);
        let viewport = Rect::from_min_size(egui::Pos2::ZERO, size);
        let root = h.ui.last_root.as_ref().unwrap();
        assert_eq!(vec2(root.width, root.height), size);
        let sidebar = h.ui.renderer.regions["Sidebar"].0;
        assert!((sidebar.bottom() - (size.y - h.ui.layout.margin)).abs() < 0.1);
        for predicate in [
            |c: &Control| matches!(c, Control::PlayPause),
            |c: &Control| matches!(c, Control::Seek),
            |c: &Control| matches!(c, Control::Next),
            |c: &Control| matches!(c, Control::Mute),
            |c: &Control| matches!(c, Control::ToggleLyrics),
        ] {
            assert!(
                viewport.contains(h.point(predicate, 0.5)),
                "offscreen player at {size:?}"
            );
        }
        let p = h.point(|c| matches!(c, Control::Navigate(Page::Search)), 0.5);
        let actions = h.click(&m, p);
        assert!(actions
            .iter()
            .any(|a| matches!(a, Intent::Navigate(Page::Search))));
        h.ui.page = Page::Sonora;
    }
}

#[test]
fn dragging_seek_previews_without_interrupting_audio_and_seeks_once_on_release() {
    let m = fixture();
    let mut h = Harness::new();
    h.frame(&m, vec![]);
    let start = h.point(|c| matches!(c, Control::Seek), 0.25);
    let end = h.point(|c| matches!(c, Control::Seek), 0.75);
    let actions = h.frame(
        &m,
        vec![
            Event::PointerMoved(start),
            Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert!(!actions.iter().any(|a| matches!(a, Intent::Seek(_))));
    for step in 1..=10 {
        let p = start + (end - start) * (step as f32 / 10.);
        let actions = h.frame(&m, vec![Event::PointerMoved(p)]);
        assert!(!actions.iter().any(|a| matches!(a, Intent::Seek(_))));
    }
    assert!((h.ui.seek_preview.unwrap() - 150.).abs() < 0.01);
    assert_eq!(m.status.position, 48.);
    let actions = h.frame(
        &m,
        vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Intent::Seek(_)))
            .count(),
        1
    );
    assert!(actions
        .iter()
        .any(|a| matches!(a,Intent::Seek(v)if (*v-150.).abs()<0.01)));
    assert!(h.ui.seek_preview.is_none());
}

#[test]
fn carousel_animation_reuses_shadows_and_stable_font_sizes() {
    let mut m = fixture();
    let mut h = Harness::new();
    h.ui.reduced_motion = false;
    h.frame(&m, vec![]);
    let before = h.ui.renderer.cache_counts();
    m.cursor = Some(1);
    m.status.track = Some(m.queue[1].clone());
    for _ in 0..8 {
        h.frame(&m, vec![]);
    }
    let after = h.ui.renderer.cache_counts();
    assert_eq!(
        before.0, after.0,
        "moving cards must reuse cached shadow masks"
    );
    assert!(
        after.1 <= before.1 + 4,
        "font atlas grew during transform-only animation: {before:?} -> {after:?}"
    );
}

#[test]
fn repeated_service_errors_refresh_the_generic_notification() {
    let mut m = fixture();
    let mut h = Harness::new();
    m.message = crate::diagnostics::USER_ERROR.into();
    m.error_generation = 1;
    h.frame(&m, vec![]);
    let first = h.ui.message_at;
    for _ in 0..80 {
        h.frame(&m, vec![]);
    }
    m.error_generation += 1;
    h.frame(&m, vec![]);
    assert!(h.ui.message_at > first + 7.);
    assert_eq!(h.ui.last_message, crate::diagnostics::USER_ERROR);
    assert!(h.ui.renderer.regions.contains_key("Toast"));
}

#[test]
fn compact_dialog_controls_are_visible_and_equalizer_lanes_do_not_overlap() {
    let mut m = fixture();
    let mut h = Harness::new();
    h.size = vec2(360., 480.);
    h.ui.overlay = Some(Overlay::Eq);
    h.frame(&m, vec![]);
    for index in 0..10 {
        let p = h.point(|c| matches!(c,Control::EqBand(i)if *i==index), 0.5);
        let hit =
            h.ui.renderer
                .hits
                .iter()
                .rev()
                .find(|h| h.contains(p))
                .unwrap();
        assert!(matches!(hit.control,Control::EqBand(i)if i==index));
    }
    let t = m.queue[0].clone();
    m.alternatives = Some((t.clone(), vec![t], vec![]));
    h.ui.overlay = Some(Overlay::Versions);
    h.frame(&m, vec![]);
    let p = h.point(|c| matches!(c, Control::VersionSelect(0)), 0.5);
    let viewport = Rect::from_min_size(egui::Pos2::ZERO, h.size);
    assert!(viewport.contains(p));
    h.click(&m, p);
    assert_eq!(h.ui.selected_version, Some(0));
}

#[test]
fn clearing_search_does_not_restore_the_previous_query() {
    let mut m = fixture();
    m.query = "Teardrop".into();
    let mut h = Harness::new();
    h.ui.page = Page::Search;
    h.frame(&m, vec![]);
    let field = h.ui.renderer.regions["Search field"].0;
    h.click(&m, field.min + vec2(64., 26.));
    let ctrl = Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    };
    h.frame(
        &m,
        vec![Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
    );
    h.frame(
        &m,
        vec![Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    h.frame(&m, vec![]);
    assert!(h.ui.query.is_empty());
}
#[test]
fn authored_assets_and_motion_endpoints_are_complete() {
    let ui = GlassUi::new();
    fn audit(n: &Node, covered: bool) {
        let covered = covered || n.asset.is_some();
        if n.kind == "VECTOR" {
            assert!(covered, "Unrendered Figma vector {}", n.id);
        }
        if let Some(asset) = &n.asset {
            assert!(assets::bytes(asset).is_some(), "Missing {asset}");
        }
        for fill in &n.fills {
            if fill["type"] == "IMAGE" {
                assert!(assets::bytes(fill["source"].as_str().unwrap()).is_some());
            }
        }
        for child in &n.children {
            audit(child, covered);
        }
    }
    for root in ui.roots.values() {
        audit(root, false);
    }
    assert_eq!(motion::ambient(0.), 0.);
    assert_eq!(motion::ambient(7.075), 1.);
    assert!(motion::ambient(14.099) < 0.001);
}

#[test]
fn every_image_slot_uses_its_original_figma_asset() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../design/native-assets.json")).unwrap();
    fn check(n: &Node, manifest: &serde_json::Value) {
        for fill in &n.fills {
            if fill["type"] == "IMAGE" {
                let source = fill["source"].as_str().unwrap();
                assert_eq!(
                    manifest[source]["imageHash"], fill["imageHash"],
                    "Wrong asset in {}",
                    n.id
                );
            }
        }
        for child in &n.children {
            check(child, manifest);
        }
    }
    for root in GlassUi::new().roots.values() {
        check(root, &manifest);
    }
}
