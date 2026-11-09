use super::*;
use serde_json::json;
pub(super) fn frame(name: &str, x: f32, y: f32, w: f32, h: f32) -> Node {
    let mut n = Node {
        name: name.into(),
        id: name.into(),
        kind: "FRAME".into(),
        visible: true,
        opacity: 1.,
        width: w,
        height: h,
        ..Default::default()
    };
    n.move_to(x, y);
    n
}
pub(super) fn fill(rgb: [f32; 3], opacity: f32) -> serde_json::Value {
    json!({"type":"SOLID","color":{"r":rgb[0],"g":rgb[1],"b":rgb[2]},"opacity":opacity})
}
pub(super) fn text(value: &str, x: f32, y: f32, w: f32, size: f32, bold: bool) -> Node {
    let mut n = Node::text_node(value, x, y, w, false);
    n.font_size = size;
    n.height = size * 1.5;
    n.line_height = json!({"unit":"PIXELS","value":size*1.5});
    n.font_name = json!({"family":"Medeo Sans","style":if bold{"SemiBold"}else{"Medium"}});
    n
}
pub(super) fn icon(asset: &str, x: f32, y: f32, size: f32) -> Node {
    let mut n = frame(asset, x, y, size, size);
    n.asset = Some(asset.into());
    if !asset.starts_with("provider-") {
        n.tint = json!({"r":0.13,"g":0.12,"b":0.15});
    }
    n
}
pub(super) fn button(
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    control: Control,
    selected: bool,
) -> Node {
    let mut n = frame(label, x, y, w, h);
    n.corner_radius = h / 2.;
    n.fills = vec![fill(
        if selected {
            [0.91, 1., 0.64]
        } else {
            [0.93, 0.925, 0.915]
        },
        1.,
    )];
    n.command = Some(control);
    let font = if label.chars().count() as f32 * 6.3 > w - 16. {
        11.
    } else {
        13.
    };
    let mut t = text(label, 8., (h - font * 1.5) / 2., w - 16., font, false);
    t.text_align_horizontal = "CENTER".into();
    n.children.push(t);
    n
}
pub(super) fn icon_button(
    asset: &str,
    x: f32,
    y: f32,
    size: f32,
    control: Control,
    selected: bool,
) -> Node {
    let mut n = button("", x, y, size, size, control, selected);
    n.id = "Icon button".into();
    n.children.clear();
    let mut glyph = icon(asset, size * 0.24, size * 0.24, size * 0.52);
    glyph.id = "Button glyph".into();
    n.children.push(glyph);
    n
}
pub(super) fn like_button(track: &Track, m: &Model, x: f32, y: f32, size: f32) -> Node {
    let mut button = icon_button("heart", x, y, size, Control::Like(track.clone()), false);
    button.children[0].move_to(size * 0.16, size * 0.16);
    button.children[0].width = size * 0.68;
    button.children[0].height = size * 0.68;
    button.fills.clear();
    if m.player_liked
        .filter(|_| {
            m.status
                .track
                .as_ref()
                .is_some_and(|t| t.key() == track.key())
        })
        .unwrap_or_else(|| m.favorites.iter().any(|f| f.key() == track.key()))
    {
        let glyph = &mut button.children[0];
        glyph.name = "Active like".into();
        glyph.tint = json!({"r":0.91,"g":1.,"b":0.64});
    }
    button
}
pub(super) fn panel(ui: &GlassUi, title: &str, w: f32, h: f32) -> (Node, Node) {
    let size = ui.layout.size;
    let w = w.min(size.x - 2. * ui.layout.margin);
    let h = h.min(size.y - 2. * ui.layout.margin);
    let mut root = frame("Runtime panel", 0., 0., size.x, size.y);
    let mut scrim = frame("Click outside to close", 0., 0., size.x, size.y);
    scrim.fills = vec![fill([0., 0., 0.], 0.2)];
    scrim.command = Some(Control::Close);
    root.children.push(scrim);
    let mut p = ui.roots["13:2407"].find("Glass dialog").unwrap().clone();
    p.children.clear();
    p.command = Some(Control::Noop);
    p.width = w;
    p.height = h;
    p.move_to((size.x - w) / 2., (size.y - h) / 2.);
    p.children.push(text(title, 28., 22., w - 100., 24., true));
    p.children
        .push(icon_button("x", w - 56., 16., 40., Control::Close, false));
    (root, p)
}
pub(super) fn body(ui: &mut GlassUi, p: &Node, total: f32) -> Node {
    let h = (p.height - 106.).max(100.);
    ui.dialog_scroll_max = (total - h).max(0.);
    let offset = ui
        .offsets
        .get("dialog-scroll")
        .copied()
        .unwrap_or(0.)
        .min(ui.dialog_scroll_max);
    let mut b = frame("Dialog content", 24., 88., p.width - 48., h);
    b.clips_content = true;
    let mut content = frame("Panel body", 0., -offset, b.width, total);
    content.clips_content = false;
    b.children.push(content);
    if ui.dialog_scroll_max > 0. {
        let track = h - 8.;
        let thumb = (track * h / total).max(28.);
        let mut scroll = frame(
            "Scroll indicator",
            b.width - 3.,
            4. + (track - thumb) * offset / ui.dialog_scroll_max,
            3.,
            thumb,
        );
        scroll.corner_radius = 1.5;
        scroll.fills = vec![fill([0.5, 0.48, 0.53], 0.35)];
        b.children.push(scroll);
    }
    b
}
fn section(p: &mut Node, value: &str, y: f32) {
    p.children.push(text(value, 4., y, p.width - 8., 12., true));
}
pub(super) fn settings(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut p) = panel(ui, "Настройки", 720., 760.);
    let mut viewport = body(ui, &p, 884.);
    let b = &mut viewport.children[0];
    let w = b.width;
    section(b, "Аккаунт и сервисы", 0.);
    b.children.push(text(
        if m.account.is_empty() {
            "Ваша музыка в одном месте"
        } else {
            &m.account
        },
        4.,
        26.,
        w,
        18.,
        true,
    ));
    for (i, asset) in [
        "provider-yandex",
        "provider-lastfm",
        "provider-soundcloud",
        "provider-hitmo",
        "provider-youtube",
    ]
    .into_iter()
    .enumerate()
    {
        let mut mark = icon(asset, 4. + i as f32 * 36., 64., 24.);
        mark.command = Some(Control::Source(connections::label_for_asset(asset).into()));
        if !m.connected[i] {
            mark.opacity = 0.25;
        }
        b.children.push(mark);
    }
    b.children.push(button(
        "Подключения",
        0.,
        104.,
        w,
        40.,
        Control::Overlay(Overlay::Sources),
        false,
    ));
    section(b, "Оформление", 172.);
    let gap = 8.;
    let bw = (w - gap * 2.) / 3.;
    for (i, (choice, label)) in [
        (ThemePreference::System, "Как в системе"),
        (ThemePreference::Light, "Светлая"),
        (ThemePreference::Dark, "Тёмная"),
    ]
    .into_iter()
    .enumerate()
    {
        b.children.push(button(
            label,
            i as f32 * (bw + gap),
            200.,
            bw,
            42.,
            Control::Theme(choice),
            m.settings.theme == choice,
        ));
    }
    section(b, "Воспроизведение", 276.);
    b.children.push(text(
        "Плавный переход между треками",
        4.,
        304.,
        w,
        16.,
        true,
    ));
    b.children.push(text(
        if m.settings.crossfade > 0. {
            "Две записи плавно сменяют друг друга"
        } else {
            "Выключен"
        },
        4.,
        330.,
        w,
        12.,
        false,
    ));
    b.children.push(button(
        "Выкл.",
        0.,
        366.,
        72.,
        36.,
        Control::CrossfadeOff,
        m.settings.crossfade == 0.,
    ));
    let mut slider = frame("Crossfade duration", 92., 366., (w - 108.).max(110.), 36.);
    slider.command = Some(Control::Crossfade);
    slider.children.push(text(
        &format!("{:0.1} с", m.settings.crossfade),
        0.,
        0.,
        slider.width,
        12.,
        true,
    ));
    let mut rail = frame("Rail", 0., 28., slider.width, 3.);
    rail.fills = vec![fill([0.5, 0.48, 0.53], 0.25)];
    rail.corner_radius = 2.;
    let mut progress = rail.clone();
    progress.name = "Progress".into();
    progress.width = slider.width * m.settings.crossfade / 12.;
    let mut thumb = frame("Thumb", progress.width - 5., 24., 10., 10.);
    thumb.corner_radius = 5.;
    thumb.fills = vec![fill([0.16, 0.14, 0.19], 1.)];
    slider.children.extend([rail, progress, thumb]);
    b.children.push(slider);
    section(
        b,
        if crate::platform::mobile() {
            "В фоновом режиме"
        } else {
            "При закрытии окна"
        },
        442.,
    );
    let bw = (w - 8.) / 2.;
    b.children.push(button(
        if crate::platform::mobile() {
            "Продолжать играть"
        } else {
            "Свернуть в трей"
        },
        0.,
        470.,
        bw,
        42.,
        Control::CloseToTray(true),
        m.settings.close_to_tray,
    ));
    b.children.push(button(
        if crate::platform::mobile() {
            "Остановить музыку"
        } else {
            "Закрыть приложение"
        },
        bw + 8.,
        470.,
        bw,
        42.,
        Control::CloseToTray(false),
        !m.settings.close_to_tray,
    ));
    b.children.push(text(
        if crate::platform::mobile() {
            "Плеер доступен на экране блокировки."
        } else {
            "Музыка продолжает играть, когда окно скрыто в трей."
        },
        4.,
        526.,
        w,
        12.,
        false,
    ));
    b.children.push(text(
        &format!("WeDex · версия {}", env!("CARGO_PKG_VERSION")),
        4.,
        836.,
        w,
        12.,
        false,
    ));
    section(b, "Версии треков", 582.);
    b.children.push(button(
        "Предпочитать оригинальные версии",
        0.,
        612.,
        w,
        42.,
        Control::PreferOriginal,
        m.settings.prefer_original,
    ));
    b.children.push(button(
        "Моё облако",
        0.,
        694.,
        w,
        44.,
        Control::Overlay(Overlay::Cloud),
        false,
    ));
    b.children.push(button(
        "Обновления",
        0.,
        754.,
        w,
        44.,
        Control::Overlay(Overlay::Updates),
        false,
    ));
    p.children.push(viewport);
    root.children.push(p);
    root
}
pub(super) fn equalizer(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut p) = panel(ui, "Эквалайзер", 940., 760.);
    let columns = if p.width < 600. { 1 } else { 3 };
    let rows = crate::equalizer::PRESETS.len().div_ceil(columns);
    let total = 410. + rows as f32 * 44.;
    let mut viewport = body(ui, &p, total);
    let b = &mut viewport.children[0];
    let w = b.width;
    b.children.push(text(
        "Диапазон ±12 дБ · Уровень — предусиление сигнала",
        0.,
        0.,
        w,
        12.,
        false,
    ));
    let lane = w / 11.;
    for index in 0..11 {
        let gain = if index == 0 {
            m.settings.eq_preamp
        } else {
            m.settings.eq[index - 1]
        };
        let control = if index == 0 {
            Control::Preamp
        } else {
            Control::EqBand(index - 1)
        };
        let mut band = frame(
            &format!("Band/{index}"),
            index as f32 * lane,
            38.,
            lane,
            236.,
        );
        band.command = Some(control);
        let mut value = text(&format!("{gain:+0.0}"), 0., 0., lane, 11., true);
        value.text_align_horizontal = "CENTER".into();
        band.children.push(value);
        let mut rail = frame("Rail", lane / 2. - 1.5, 28., 3., 160.);
        rail.corner_radius = 1.5;
        rail.fills = vec![fill([0.5, 0.48, 0.53], 0.22)];
        let mut zero = frame("Zero", lane / 2. - 4., 107., 8., 1.);
        zero.fills = vec![fill([0.5, 0.48, 0.53], 0.6)];
        let mut thumb = frame(
            "Thumb",
            lane / 2. - 6.,
            28. + (12. - gain) / 24. * 160. - 6.,
            12.,
            12.,
        );
        thumb.corner_radius = 6.;
        thumb.fills = vec![fill([0.16, 0.14, 0.19], 1.)];
        let frequency = if index == 0 {
            "Уровень".into()
        } else {
            let hz = crate::equalizer::FREQUENCIES[index - 1];
            if hz >= 1000. {
                format!("{}k", hz / 1000.)
            } else {
                format!("{hz:0.0}")
            }
        };
        let mut label = text(
            &frequency,
            0.,
            204.,
            lane,
            if lane < 38. { 8. } else { 11. },
            false,
        );
        label.text_align_horizontal = "CENTER".into();
        band.children.extend([rail, zero, thumb, label]);
        b.children.push(band);
    }
    b.children.push(text("+12 дБ", 0., 284., w, 10., false));
    let mut min = text("−12 дБ", 0., 284., w, 10., false);
    min.text_align_horizontal = "RIGHT".into();
    b.children.push(min);
    section(b, &format!("Пресеты · {}", m.settings.eq_preset), 320.);
    let bw = (w - 8. * (columns - 1) as f32) / columns as f32;
    for (i, preset) in crate::equalizer::PRESETS.iter().enumerate() {
        b.children.push(button(
            preset.name,
            (i % columns) as f32 * (bw + 8.),
            354. + (i / columns) as f32 * 44.,
            bw,
            36.,
            Control::Preset(i),
            m.settings.eq_preset == preset.name,
        ));
    }
    p.children.push(viewport);
    root.children.push(p);
    root
}
pub(super) fn lyrics(ui: &mut GlassUi, ctx: &egui::Context, m: &Model, zen: bool) -> Node {
    super::lyrics_view::show(ui, ctx, m, zen)
}
pub(super) fn volume(ui: &GlassUi, m: &Model, anchor: Rect) -> Node {
    let mut root = frame("Volume popup", 0., 0., ui.layout.size.x, ui.layout.size.y);
    let w = 56.;
    let h = 174.;
    let x = (anchor.center().x - w / 2.).clamp(8., ui.layout.size.x - w - 8.);
    let y = (anchor.min.y - h - 12.).max(8.);
    let mut p = frame("Glass dialog", x, y, w, h);
    p.command = Some(Control::Noop);
    p.corner_radius = 24.;
    p.effects = ui.roots["13:2407"]
        .find("Glass dialog")
        .unwrap()
        .effects
        .clone();
    p.fills = vec![fill([1., 1., 1.], 0.9)];
    let mut slider = frame("Vertical volume", 12., 16., 32., 120.);
    slider.command = Some(Control::VerticalVolume);
    let mut rail = frame("Rail", 14., 0., 4., 120.);
    rail.corner_radius = 2.;
    rail.fills = vec![fill([0.5, 0.48, 0.53], 0.25)];
    let mut progress = rail.clone();
    progress.name = "Progress".into();
    progress.height = 120. * m.settings.volume;
    progress.move_to(14., 120. - progress.height);
    let mut thumb = frame("Thumb", 10., (1. - m.settings.volume) * 120. - 4., 12., 12.);
    thumb.corner_radius = 6.;
    thumb.fills = vec![fill([0.16, 0.14, 0.19], 1.)];
    slider.children.extend([rail, progress, thumb]);
    p.children.push(slider);
    let mut value = text(
        &format!("{:0.0}%", m.settings.volume * 100.),
        0.,
        142.,
        w,
        11.,
        true,
    );
    value.text_align_horizontal = "CENTER".into();
    p.children.push(value);
    root.children.push(p);
    root
}
pub(super) fn zen(
    ui: &mut GlassUi,
    ctx: &egui::Context,
    m: &Model,
    pointer: Option<egui::Pos2>,
) -> Node {
    let size = ui.layout.size;
    let mut root = frame("Zen", 0., 0., size.x, size.y);
    if let Some(ambient) = ui.roots["8:157"].find("Ambient light") {
        let mut ambient = ambient.clone();
        ambient.width = size.x;
        ambient.height = size.y;
        root.children.push(ambient);
    }
    if ui.layout.mobile {
        return super::mobile_player::zen(ui, ctx, m, root);
    }
    let lyrics = m.settings.lyrics_enabled && size.x > 1000.;
    let stacked = m.settings.lyrics_enabled && !lyrics;
    let center = if lyrics { size.x * 0.25 } else { size.x / 2. };
    let side = (if stacked {
        size.x * 0.32
    } else if lyrics {
        size.x * 0.23
    } else if size.x <= 1000. {
        size.x - 48.
    } else {
        size.x * 0.34
    })
    .min(size.y * if stacked { 0.23 } else { 0.46 })
    .clamp(100., 500.);
    let top = if stacked {
        48.
    } else {
        ((size.y - side - 160.) / 2.).max(if size.y < 600. { 88. } else { 0. })
    };
    let cover_rect = Rect::from_min_size(pos2(center - side / 2., top), vec2(side, side));
    let current_track = ui.current(m).cloned();
    let current = current_track.as_ref();
    let mut cover = frame("Zen artwork", cover_rect.min.x, top, side, side);
    cover.corner_radius = 16.;
    cover.fills = vec![
        json!({"type":"IMAGE","source":current.map(GlassUi::source).unwrap_or_else(||"missing".into())}),
    ];
    let hovered = pointer.is_some_and(|p| cover_rect.contains(p));
    let controls_opacity = ui.zen_controls.value(
        if hovered || crate::platform::mobile() {
            1.
        } else {
            0.
        },
        ctx.input(|i| i.time),
        ui.reduced_motion,
    );
    if controls_opacity > 0.001 {
        let mut shade = frame("Hover controls", 0., 0., side, side);
        shade.opacity = controls_opacity;
        shade.corner_radius = 16.;
        shade.fills = vec![fill([0., 0., 0.], 0.4)];
        let button_size = (side * 0.13).clamp(18., 50.);
        let gap = (side * 0.016).clamp(4., 8.);
        let center_y = side / 2. - button_size / 2.;
        for (i, (asset, control)) in [
            ("shuffle", Control::Shuffle),
            ("skip-back", Control::Previous),
            ("play", Control::PlayPause),
            ("skip-forward", Control::Next),
            ("repeat", Control::Repeat),
        ]
        .into_iter()
        .enumerate()
        {
            let asset = if i == 2 && m.status.playing {
                "pause"
            } else {
                asset
            };
            shade.children.push(icon_button(
                asset,
                side / 2. + (i as f32 - 2.) * (button_size + gap) - button_size / 2.,
                center_y,
                button_size,
                control,
                i == 2 || (i == 4 && m.settings.repeat_current) || (i == 0 && ui.shuffle),
            ));
        }
        shade.children.push(icon_button(
            "list-music",
            side - button_size - 16.,
            16.,
            button_size,
            Control::Overlay(Overlay::Queue),
            false,
        ));
        shade.children.push(icon_button(
            "lyrics",
            side / 2. - button_size / 2.,
            side - button_size - 16.,
            button_size,
            Control::ToggleLyrics,
            m.settings.lyrics_enabled,
        ));
        if let Some(t) = current {
            shade.children.push(like_button(
                t,
                m,
                side - button_size - 16.,
                side - button_size - 16.,
                button_size,
            ));
            shade.children.push(icon_button(
                "ellipsis",
                16.,
                side - button_size - 16.,
                button_size,
                Control::Row(t.clone()),
                false,
            ));
        }
        if !hovered && !crate::platform::mobile() {
            shade.visit(&mut |n| n.command = None);
        }
        cover.children.push(shade);
    }
    root.children.push(cover);
    for (value, y, font, bold) in [
        (
            current
                .map(|t| t.title.as_str())
                .unwrap_or("Выберите музыку"),
            top + side + 24.,
            20.,
            true,
        ),
        (
            current.map(|t| t.artist.as_str()).unwrap_or(""),
            top + side + 54.,
            14.,
            false,
        ),
    ] {
        let label_w = if stacked { size.x - 32. } else { side };
        let mut t = text(value, center - label_w / 2., y, label_w, font, bold);
        t.text_align_horizontal = "CENTER".into();
        root.children.push(t);
    }
    let mut seek = frame("Zen seek", center - side / 2., top + side + 100., side, 20.);
    seek.command = Some(Control::Seek);
    let mut rail = frame("Rail", 0., 8., side, 4.);
    rail.corner_radius = 2.;
    rail.fills = vec![fill([0.5, 0.48, 0.53], 0.3)];
    let mut progress = rail.clone();
    progress.name = "Progress".into();
    progress.width = side
        * (ui.seek_preview.unwrap_or(m.status.position) / m.status.duration.max(1.)).clamp(0., 1.);
    seek.children.extend([rail, progress]);
    root.children.push(seek);
    if m.settings.lyrics_enabled {
        root.children
            .extend(self::lyrics(ui, ctx, m, true).children);
    }
    root.children.push(icon_button(
        "chevron-down",
        size.x - 88.,
        40.,
        48.,
        Control::Zen(false),
        false,
    ));
    root
}
