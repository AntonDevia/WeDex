use super::panels::{fill, frame, icon_button, text};
use super::*;

pub(super) fn show(ui: &mut GlassUi, ctx: &egui::Context, m: &Model, zen: bool) -> Node {
    show_in(ui, ctx, m, zen, None)
}
pub(super) fn show_in(
    ui: &mut GlassUi,
    ctx: &egui::Context,
    m: &Model,
    zen: bool,
    area: Option<Rect>,
) -> Node {
    let size = ui.layout.size;
    if ui.lyrics_manual_layout != Some((zen, size)) {
        ui.lyrics_manual_layout = Some((zen, size));
        ui.lyrics_manual_until = -1.;
        ui.lyrics_manual_index = None;
        ui.lyrics_wheel_remainder = 0.;
        ui.lyrics_manual_visibility = transitions::Tween::new(0.);
        ui.lyrics_motion = Default::default();
    }
    let stacked = zen && size.x <= 1000.;
    let (x, y, w, h) = if let Some(area) = area {
        (area.min.x, area.min.y, area.width(), area.height())
    } else if stacked {
        let side = (size.x * 0.32).min(size.y * 0.23);
        let y = 48. + side + 148.;
        (16., y, size.x - 32., (size.y - y - 16.).max(64.))
    } else if zen {
        (size.x * 0.48, 32., size.x * 0.50, size.y - 64.)
    } else {
        let w = (size.x - ui.layout.surface_x - 24.).clamp(200., 460.);
        (
            size.x - w - ui.layout.margin,
            104.,
            w,
            (ui.layout.player_y - 128.).max(120.),
        )
    };
    ui.lyrics_scroll_max = 0.;
    let mut root = frame("Lyrics surface", 0., 0., size.x, size.y);
    let mut surface = frame(if zen { "Zen lyrics" } else { "Glass dialog" }, x, y, w, h);
    surface.command = Some(Control::Noop);
    if !zen {
        surface.corner_radius = 28.;
        surface.fills = vec![fill([1., 1., 1.], 0.88)];
        surface.effects = ui.roots["13:2407"]
            .find("Glass dialog")
            .unwrap()
            .effects
            .clone();
        surface
            .children
            .push(text("Текст песни", 24., 20., w - 80., 20., true));
        surface.children.push(icon_button(
            "x",
            w - 56.,
            14.,
            36.,
            Control::ToggleLyrics,
            false,
        ));
    }
    let padding = if zen { 8. } else { 24. };
    let mut content = frame(
        "Lyrics content",
        padding,
        if zen { 0. } else { 68. },
        w - padding * 2.,
        if zen { h } else { (h - 92.).max(32.) },
    );
    content.clips_content = true;
    let mut base_font = if zen {
        (size.x * 0.026).clamp(24., 52.)
    } else {
        17.
    };
    let synced = m
        .lyrics
        .as_ref()
        .is_some_and(|lyrics| !lyrics.lines.is_empty());
    if stacked && synced {
        if let Some((_, fitted)) = ui
            .lyrics_fitted_font
            .filter(|(viewport, _)| *viewport == size)
        {
            base_font = fitted;
        } else {
            let lyrics = m.lyrics.as_ref().unwrap();
            for _ in 0..8 {
                let maximum = base_font * 2.;
                let rows = lyrics
                    .lines
                    .iter()
                    .map(|line| {
                        let job = egui::text::LayoutJob::simple(
                            line.text.clone(),
                            FontId::new(maximum * 2., FontFamily::Name("medeo-semibold".into())),
                            Color32::BLACK,
                            content.width * 2.,
                        );
                        ctx.fonts(|f| f.layout_job(job).rows.len()).max(1)
                    })
                    .max()
                    .unwrap_or(1);
                if rows as f32 * maximum * 1.35 <= content.height * 0.9 {
                    break;
                }
                base_font *= 0.85;
            }
            ui.lyrics_fitted_font = Some((size, base_font));
        }
    }
    // Render at the maximum size and animate the transform, keeping the glyph
    // atlas and line breaks stable throughout each zoom.
    let font = base_font * if synced { 2. } else { 1. };
    let row = |value: &str| {
        let mut job = egui::text::LayoutJob::simple(
            value.into(),
            FontId::new(font * 2., FontFamily::Name("medeo-semibold".into())),
            Color32::BLACK,
            content.width * 2.,
        );
        job.sections[0].format.line_height = Some(font * 2.7);
        let rows = ctx.fonts(|f| f.layout_job(job).rows.len());
        let mut n = text(value, 0., 0., content.width, font, true);
        n.name = "Lyric line".into();
        n.line_height = serde_json::json!({"unit":"PIXELS","value":font*1.35});
        n.height = rows.max(1) as f32 * font * 1.35;
        n.text_align_horizontal = "CENTER".into();
        n
    };
    let now = ctx.input(|i| i.time);
    if let Some(lyrics) = &m.lyrics {
        if !lyrics.lines.is_empty() {
            let active = lyrics.active(m.status.position).unwrap_or(0);
            let hovered = ui.overlay.is_none()
                && ctx.input(|i| {
                    i.pointer.hover_pos().is_some_and(|p| {
                        Rect::from_min_size(
                            pos2(x + content.x, y + content.y),
                            vec2(content.width, content.height),
                        )
                        .contains(p)
                    })
                });
            // Consume physical wheel events, not egui's inertial pixel tail.
            // Precision devices accumulate fractional rows without ever
            // leaving the text parked between two lyric lines.
            let rows = if hovered {
                ctx.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|event| match event {
                            egui::Event::MouseWheel { unit, delta, .. } if delta.y != 0. => Some(
                                -delta.y
                                    * match unit {
                                        egui::MouseWheelUnit::Line => 1.,
                                        egui::MouseWheelUnit::Point => 1. / 40.,
                                        egui::MouseWheelUnit::Page => {
                                            (content.height / (base_font * 1.9)).max(1.)
                                        }
                                    },
                            ),
                            _ => None,
                        })
                        .sum::<f32>()
                }) - ui.touch_delta / 40.
            } else {
                0.
            };
            if rows != 0. {
                if now >= ui.lyrics_manual_until {
                    ui.lyrics_manual_index = Some(active);
                    ui.lyrics_wheel_remainder = 0.;
                }
                ui.lyrics_wheel_remainder += rows;
                let steps = ui.lyrics_wheel_remainder.trunc() as isize;
                ui.lyrics_wheel_remainder -= steps as f32;
                let index = ui.lyrics_manual_index.unwrap_or(active);
                let next = index
                    .saturating_add_signed(steps)
                    .min(lyrics.lines.len() - 1);
                ui.lyrics_manual_index = Some(next);
                if (next == 0 && rows < 0.) || (next + 1 == lyrics.lines.len() && rows > 0.) {
                    ui.lyrics_wheel_remainder = 0.;
                }
                ui.lyrics_manual_until = now + 3.;
            }
            let manual = now < ui.lyrics_manual_until;
            let manual_visibility = ui.lyrics_manual_visibility.value(
                if manual { 1. } else { 0. },
                now,
                ui.reduced_motion,
            );
            if manual {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    ui.lyrics_manual_until - now,
                ));
            }
            let mut nodes = Vec::new();
            let mut y = 0.;
            let mut final_y = 0.;
            let mut target = 0.;
            for (index, line) in lyrics.lines.iter().enumerate() {
                let mut n = row(if line.text.is_empty() {
                    "♪"
                } else {
                    &line.text
                });
                if index == active {
                    target = if zen {
                        final_y + n.height / 2. - content.height * 0.43
                    } else {
                        (final_y + n.height / 2. - content.height * 0.43).max(0.)
                    };
                }
                let emphasis = ui.lyrics_emphasis.entry(index).or_default().value(
                    if index == active && !manual { 1. } else { 0. },
                    now,
                    ui.reduced_motion || manual,
                );
                let scale = 0.5 + 0.5 * emphasis;
                n.move_to((content.width - n.width * scale) / 2., y);
                n.relative_transform = Some([[scale, 0., n.x], [0., scale, n.y]]);
                let gap = if zen { base_font * 0.55 } else { 24. };
                y += n.height * scale + gap;
                final_y += n.height * if index == active { 1. } else { 0.5 } + gap;
                n.opacity = if zen {
                    0.16 + 0.84 * emphasis
                } else {
                    0.43 + 0.57 * emphasis
                };
                n.opacity += (1. - n.opacity) * manual_visibility;
                n.command = Some(Control::LyricSeek(line.time));
                nodes.push(n);
            }
            if manual {
                if let Some(n) = ui.lyrics_manual_index.and_then(|index| nodes.get(index)) {
                    target = n.y + n.height * 0.25 - content.height * 0.43;
                }
            }
            let scroll = ui
                .lyrics_motion
                .value(target, now, ui.reduced_motion || manual);
            for mut n in nodes {
                n.move_to(n.x, n.y - scroll);
                let height = n.height * n.relative_transform.unwrap()[0][0];
                if zen && !manual {
                    let edge = (n.y + height / 2.).min(content.height - n.y - height / 2.);
                    n.opacity *= (edge / (base_font * 1.3)).clamp(0., 1.);
                }
                if n.y + height >= 0. && n.y < content.height {
                    content.children.push(n);
                }
            }
        } else {
            let value = if lyrics.instrumental {
                "Инструментальная композиция"
            } else if lyrics.plain.is_empty() {
                "Текст пока не найден"
            } else {
                &lyrics.plain
            };
            let mut nodes = Vec::new();
            let mut y = 0.;
            for line in value.lines() {
                let mut n = row(line);
                n.move_to(0., y);
                y += n.height + font * 0.7;
                nodes.push(n);
            }
            ui.lyrics_scroll_max = (y - content.height).max(0.);
            let offset = ui
                .offsets
                .get("lyrics-scroll")
                .copied()
                .unwrap_or(0.)
                .min(ui.lyrics_scroll_max);
            for mut n in nodes {
                n.move_to(0., n.y - offset);
                content.children.push(n);
            }
        }
    } else {
        let mut n = row(if m.lyrics_loading {
            "Ищем текст…"
        } else {
            "Текст пока не найден"
        });
        if zen {
            n.move_to(0., content.height * 0.43);
            n.opacity = 0.55;
        }
        content.children.push(n);
    }
    surface.children.push(content);
    root.children.push(surface);
    root
}
