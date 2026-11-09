//! Touch player: a quiet mini-player and a full player with clean artwork.
use super::{panels::*, *};

pub(super) struct Geometry {
    pub art: Rect,
    pub metadata: Rect,
    pub seek: Rect,
    pub controls: Rect,
    pub secondary: Rect,
    pub radio: Option<Rect>,
}
impl Geometry {
    pub fn new(size: egui::Vec2) -> Self {
        let rect = |x, y, w, h| Rect::from_min_size(pos2(x, y), vec2(w, h));
        if size.x > size.y {
            let side = (size.x * 0.40).min(size.y - 128.).max(100.);
            let x = side + 48.;
            let w = (size.x - x - 24.).max(260.);
            Self {
                art: rect(24., 104., side, side),
                metadata: rect(x, 104., w, 54.),
                seek: rect(x, 170., w, 40.),
                controls: rect(x, 226., w, 64.),
                secondary: rect(x, 306., w, 44.),
                radio: (size.y >= 450.).then(|| rect(x, 374., w, 56.)),
            }
        } else {
            let w = (size.x - 48.).min(560.);
            let left = (size.x - w) / 2.;
            let compact = size.y < 620.;
            let top = if compact { 88. } else { 104. };
            let side = w.min(size.y - if compact { 350. } else { 470. }).max(100.);
            let bottom = top + side;
            Self {
                art: rect((size.x - side) / 2., top, side, side),
                metadata: rect(left, bottom + if compact { 12. } else { 20. }, w, 54.),
                seek: rect(left, bottom + if compact { 76. } else { 90. }, w, 40.),
                controls: rect(left, bottom + if compact { 126. } else { 146. }, w, 64.),
                secondary: rect(left, bottom + if compact { 202. } else { 230. }, w, 44.),
                radio: (!compact).then(|| rect(left, bottom + 294., w, 56.)),
            }
        }
    }
}

fn plain(asset: &str, x: f32, y: f32, size: f32, control: Control) -> Node {
    let mut n = icon_button(asset, x, y, size, control, false);
    n.name = format!("Zen mobile/{asset}");
    n.fills.clear();
    n
}
fn artwork(source: &str, name: &str, rect: Rect, radius: f32) -> Node {
    let mut n = frame(name, rect.min.x, rect.min.y, rect.width(), rect.height());
    n.corner_radius = radius;
    n.clips_content = true;
    n.fills = vec![serde_json::json!({"type":"IMAGE","source":source})];
    n
}
fn seek(ui: &GlassUi, m: &Model, rect: Rect) -> Node {
    let w = rect.width();
    let mut n = frame("Zen seek", rect.min.x, rect.min.y, w, rect.height());
    n.command = Some(Control::Seek);
    let position = ui.seek_preview.unwrap_or(m.status.position);
    let fraction = (position / m.status.duration.max(1.)).clamp(0., 1.);
    let mut rail = frame("Rail", 0., 10., w, 3.);
    rail.corner_radius = 1.5;
    rail.fills = vec![fill([0.5, 0.48, 0.53], 0.3)];
    let mut progress = rail.clone();
    progress.name = "Progress".into();
    progress.width = w * fraction;
    progress.fills = vec![fill([0.2, 0.18, 0.23], 0.95)];
    let mut thumb = frame("Thumb", (w - 10.) * fraction, 6.5, 10., 10.);
    thumb.corner_radius = 5.;
    thumb.fills = progress.fills.clone();
    n.children.extend([rail, progress, thumb]);
    for (value, right) in [(position, false), (m.status.duration, true)] {
        let seconds = value.max(0.) as u64;
        let mut label = text(
            &format!("{}:{:02}", seconds / 60, seconds % 60),
            0.,
            20.,
            w,
            11.,
            false,
        );
        label.opacity = 0.6;
        if right {
            label.text_align_horizontal = "RIGHT".into();
        }
        n.children.push(label);
    }
    n
}

pub(super) fn zen(ui: &mut GlassUi, ctx: &egui::Context, m: &Model, mut root: Node) -> Node {
    let size = ui.layout.size;
    let g = Geometry::new(size);
    let current = ui.current(m).cloned();
    let source = current
        .as_ref()
        .map(GlassUi::source)
        .unwrap_or_else(|| "missing".into());
    root.children
        .push(plain("chevron-down", 12., 44., 44., Control::Zen(false)));
    let mut title = text(
        if let Some(device) = ui.cloud_view.remote_device() {
            &device.name
        } else if ui.page == Page::Sonora {
            "Sonora"
        } else {
            "Ваша музыка"
        },
        72.,
        56.,
        size.x - 144.,
        14.,
        true,
    );
    title.name = "Zen mobile/source".into();
    title.text_align_horizontal = "CENTER".into();
    root.children.push(title);
    root.children.push(plain(
        "headphones",
        size.x - 104.,
        44.,
        44.,
        Control::Overlay(Overlay::Devices),
    ));
    root.children.push(plain(
        "list-music",
        size.x - 56.,
        44.,
        44.,
        Control::Overlay(Overlay::Queue),
    ));

    if m.settings.lyrics_enabled {
        root.children
            .extend(super::lyrics_view::show_in(ui, ctx, m, true, Some(g.art)).children);
    } else {
        root.children
            .push(artwork(&source, "Zen artwork", g.art, 16.));
    }
    let mut metadata = frame(
        "Zen mobile/metadata",
        g.metadata.min.x,
        g.metadata.min.y,
        g.metadata.width(),
        g.metadata.height(),
    );
    metadata.children.push(artwork(
        &source,
        "Zen mobile/thumbnail",
        Rect::from_min_size(pos2(0., 3.), vec2(44., 44.)),
        22.,
    ));
    let width = metadata.width - 112.;
    metadata.children.push(text(
        current
            .as_ref()
            .map(|t| t.title.as_str())
            .unwrap_or("Выберите музыку"),
        58.,
        0.,
        width,
        18.,
        true,
    ));
    let mut artist = text(
        current
            .as_ref()
            .map(|t| t.artist.as_str())
            .unwrap_or("Ваш следующий любимый трек"),
        58.,
        28.,
        width,
        13.,
        false,
    );
    artist.opacity = 0.65;
    metadata.children.push(artist);
    if let Some(track) = &current {
        metadata.children.push(plain(
            "ellipsis",
            metadata.width - 44.,
            3.,
            44.,
            Control::Row(track.clone()),
        ));
    }
    root.children.push(metadata);
    root.children.push(seek(ui, m, g.seek));

    let w = g.controls.width();
    let step = (w * 0.205).min(74.);
    let mut controls = frame(
        "Zen mobile/transport",
        g.controls.min.x,
        g.controls.min.y,
        w,
        64.,
    );
    for (asset, control, center, selected) in [
        ("shuffle", Control::Shuffle, 22., ui.shuffle),
        ("skip-back", Control::Previous, w / 2. - step, false),
        ("skip-forward", Control::Next, w / 2. + step, false),
    ] {
        let mut button = plain(asset, center - 22., 10., 44., control);
        if selected {
            button.children[0].name = "Active like".into();
            button.children[0].tint = serde_json::json!({"r":0.91,"g":1.,"b":0.64});
        }
        controls.children.push(button);
    }
    let mut play = icon_button(
        if m.status.playing { "pause" } else { "play" },
        w / 2. - 32.,
        0.,
        64.,
        Control::PlayPause,
        true,
    );
    play.name = "Zen mobile/play".into();
    controls.children.push(play);
    controls.children.push(if let Some(track) = &current {
        like_button(track, m, w - 44., 10., 44.)
    } else {
        plain("heart", w - 44., 10., 44., Control::Noop)
    });
    root.children.push(controls);

    let mut secondary = frame(
        "Zen mobile/secondary",
        g.secondary.min.x,
        g.secondary.min.y,
        g.secondary.width(),
        44.,
    );
    let width = secondary.width;
    for (asset, control, x, selected) in [
        ("repeat", Control::Repeat, 0., m.settings.repeat_current),
        (
            "lyrics",
            Control::ToggleLyrics,
            width / 2. - 36.,
            m.settings.lyrics_enabled,
        ),
        (
            "sliders-horizontal",
            Control::Overlay(Overlay::Eq),
            width - 72.,
            false,
        ),
    ] {
        let mut pill = button("", x, 0., 72., 44., control, selected);
        pill.name = format!("Zen mobile/{asset}");
        pill.children.clear();
        pill.children.push(icon(asset, 26., 12., 20.));
        secondary.children.push(pill);
    }
    root.children.push(secondary);
    if let Some(rect) = g.radio {
        let mut banner = frame(
            "Zen mobile/radio",
            rect.min.x,
            rect.min.y,
            rect.width(),
            rect.height(),
        );
        banner.corner_radius = 20.;
        banner.clips_content = true;
        banner.fills = vec![fill([1., 1., 1.], 0.24)];
        banner.strokes = vec![fill([0.5, 0.48, 0.53], 0.12)];
        banner.stroke_weight = 1.;
        banner.command = current.as_ref().map(|t| Control::Radio(t.clone()));
        if let Some(template) = ui.roots["8:157"].find("Floating player") {
            banner.effects = template.effects.clone();
        }
        for (value, y, font, bold) in [
            ("Sonora по треку", 8., 11., false),
            (
                current
                    .as_ref()
                    .map(|t| t.title.as_str())
                    .unwrap_or("Выберите музыку"),
                25.,
                14.,
                true,
            ),
        ] {
            let mut label = text(value, 16., y, rect.width() - 32., font, bold);
            label.text_align_horizontal = "CENTER".into();
            if !bold {
                label.opacity = 0.65;
            }
            banner.children.push(label);
        }
        root.children.push(banner);
    }
    root
}
