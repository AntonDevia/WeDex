//! Visual operation feedback lives on the control that is waiting, not in a toast.
use super::*;
use egui::{Painter, Shape, Stroke};
#[derive(Default)]
pub(super) struct Feedback {
    pub waiting_since: Option<f64>,
    pub waiting: bool,
    pub visible: f32,
    opacity: transitions::Tween,
    connection: transitions::Tween,
}
pub(super) fn busy(m: &Model) -> bool {
    m.resolving || m.status.loading || m.status.buffering
}
fn region(ui: &GlassUi, name: &str) -> Option<Rect> {
    ui.renderer
        .regions
        .get(name)
        .map(|(b, c)| b.intersect(*c))
        .filter(|r| r.is_positive())
}
fn ring(p: &Painter, center: egui::Pos2, r: f32, time: f64, color: Color32) {
    p.circle_stroke(center, r, Stroke::new(2_f32, color.gamma_multiply(0.15)));
    let angle = time as f32 * 4.8;
    let length = 2.4 + (time as f32 * 2.).sin() * 0.8;
    let points = (0..32)
        .map(|i| center + egui::Vec2::angled(angle + i as f32 / 31. * length) * r)
        .collect();
    p.add(Shape::line(points, Stroke::new(2.4_f32, color)));
}
fn shimmer(p: &Painter, rect: Rect, time: f64, dark: bool) {
    let pulse = ((time as f32 * 2.8 + rect.top() * 0.002).sin() * 0.5 + 0.5) * 7.;
    let base = if dark {
        Color32::from_rgb(
            (33. + pulse) as u8,
            (30. + pulse) as u8,
            (38. + pulse) as u8,
        )
    } else {
        Color32::from_rgb(
            (232. + pulse) as u8,
            (229. + pulse) as u8,
            (235. + pulse) as u8,
        )
    };
    p.rect_filled(rect, 8., base);
}
pub(super) fn paint(ui: &mut GlassUi, ctx: &egui::Context, m: &Model, p: &Painter) {
    let now = ctx.input(|i| i.time);
    let waiting = busy(m) || (m.sonora_loading && m.status.track.is_none());
    if waiting != ui.feedback.waiting {
        ui.feedback.waiting = waiting;
        ui.feedback.waiting_since = waiting.then_some(now);
    }
    let target = waiting && ui.feedback.waiting_since.is_some_and(|at| now - at >= 0.12);
    ui.feedback.visible =
        ui.feedback
            .opacity
            .value(if target { 1. } else { 0. }, now, ui.reduced_motion);
    let visible = ui.feedback.visible > 0.005;
    let ink = if ui.dark {
        Color32::from_rgb(232, 255, 163)
    } else {
        Color32::from_rgb(84, 110, 40)
    };
    if visible && ui.overlay.is_none() {
        let button = ui
            .renderer
            .hits
            .iter()
            .rev()
            .find(|h| matches!(h.control, Control::PlayPause))
            .map(|h| h.transform.bounds(h.size.x, h.size.y));
        if let Some(rect) = button {
            ui.renderer
                .regions
                .insert("Player progress indicator".into(), (rect, p.clip_rect()));
            ring(
                p,
                rect.center(),
                (rect.width() * 0.23).clamp(10., 17.),
                now,
                if ui.zen {
                    Color32::from_rgb(30, 28, 34).gamma_multiply(ui.feedback.visible)
                } else {
                    ink.gamma_multiply(ui.feedback.visible)
                },
            );
        }
        if let Some(rect) = region(ui, "Zen seek").or_else(|| region(ui, "Timeline")) {
            let width = rect.width() * 0.20;
            let x = rect.left() + (rect.width() - width) * ((now * 1.1).sin() * 0.5 + 0.5) as f32;
            p.line_segment(
                [pos2(x, rect.top() + 10.), pos2(x + width, rect.top() + 10.)],
                Stroke::new(2_f32, ink),
            );
        }
        if m.sonora_loading && m.status.track.is_none() {
            if let Some(rect) = region(ui, "Album/sonora-start") {
                ring(
                    p,
                    pos2(rect.center().x, rect.top() + rect.width() * 0.48),
                    22.,
                    now,
                    ink,
                );
            }
        }
    }
    let loading_list = if ui.page == Page::Playlist && m.loading_playlist {
        Some("list/playlist")
    } else if ui.page == Page::Search && m.searching {
        Some("list/results")
    } else if ui.page == Page::Artist && m.artist.as_ref().is_some_and(|a| a.loading) {
        Some("list/artist")
    } else if m.loading_library && m.favorites.is_empty() {
        Some("list/favorites")
    } else {
        None
    };
    if ui.overlay.is_none() && !ui.zen {
        if let Some(rect) = loading_list.and_then(|name| region(ui, name)) {
            let empty = match ui.page {
                Page::Search => m.results.is_empty(),
                Page::Artist => m.artist.as_ref().is_none_or(|a| a.tracks.is_empty()),
                _ => m.favorites.is_empty(),
            };
            if empty {
                let clipped = p.with_clip_rect(rect.intersect(p.clip_rect()));
                for i in 0..8 {
                    let y = rect.top() + i as f32 * 68.;
                    shimmer(
                        &clipped,
                        Rect::from_min_size(pos2(rect.left() + 4., y + 10.), vec2(44., 44.)),
                        now,
                        ui.dark,
                    );
                    for (j, w) in [(0, 0.65), (1, 0.38)] {
                        shimmer(
                            &clipped,
                            Rect::from_min_size(
                                pos2(rect.left() + 62., y + 14. + j as f32 * 20.),
                                vec2((rect.width() - 90.) * w, 10.),
                            ),
                            now,
                            ui.dark,
                        );
                    }
                }
            } else {
                shimmer(
                    p,
                    Rect::from_min_size(rect.left_top(), vec2(rect.width(), 3.)),
                    now,
                    ui.dark,
                );
            }
        }
    }
    if ui.cloud_view.transport_pending && !visible {
        if let Some(rect) = region(ui, "Zen seek").or_else(|| region(ui, "Floating player")) {
            shimmer(
                p,
                Rect::from_min_size(
                    pos2(rect.left() + 8., rect.bottom() - 3.),
                    vec2(rect.width() - 16., 2.),
                ),
                now,
                ui.dark,
            );
        }
    }
    if m.downloads.busy {
        if let Some(rect) = region(ui, "Favorite downloads") {
            ring(p, rect.center(), 15., now, Color32::from_rgb(30, 28, 34));
        }
    }
    if m.loading_library {
        if let Some(rect) = region(ui, "button/refresh-cw") {
            ring(p, rect.center(), 11., now, ink);
        }
    }
    if ui.overlay == Some(Overlay::Updates) && m.updates.busy {
        if let Some(rect) = region(ui, "Glass dialog") {
            ring(p, pos2(rect.right() - 82., rect.top() + 36.), 10., now, ink);
        }
    }
    if ui.cloud_view.library_enabled
        && ui.page == Page::Favorites
        && !ui.zen
        && ui.overlay.is_none()
    {
        let center = pos2(ui.layout.main_x + 164., 68.);
        ui.renderer.regions.insert(
            "Library sync indicator".into(),
            (
                Rect::from_center_size(center, vec2(24., 24.)),
                p.clip_rect(),
            ),
        );
        if ui.cloud_view.library_syncing {
            ring(p, center, 8., now, ink.gamma_multiply(0.8));
        } else if ui.cloud_view.library_pending || !ui.cloud_view.connected {
            p.circle_stroke(center, 7., Stroke::new(1.6_f32, ink.gamma_multiply(0.6)));
            p.line_segment([center + vec2(0., -4.), center], Stroke::new(1.6_f32, ink));
            p.line_segment([center, center + vec2(3., 1.)], Stroke::new(1.6_f32, ink));
        } else {
            p.add(Shape::line(
                vec![
                    center + vec2(-5., 0.),
                    center + vec2(-1., 4.),
                    center + vec2(6., -4.),
                ],
                Stroke::new(1.8_f32, ink.gamma_multiply(0.7)),
            ));
        }
    }
    let disconnected = m.network != crate::network::State::Online
        || m.settings.offline_only
        || ui
            .cloud_view
            .remote_device()
            .is_some_and(|d| !d.online || !ui.cloud_view.connected);
    let connection_alpha =
        ui.feedback
            .connection
            .value(if disconnected { 1. } else { 0. }, now, ui.reduced_motion);
    if connection_alpha > 0.005 {
        let center = if ui.zen {
            pos2(70., 66.)
        } else {
            pos2(ui.layout.main_x + 125., 68.)
        };
        let color = (if ui.dark {
            Color32::from_rgb(192, 188, 199)
        } else {
            Color32::from_rgb(109, 102, 116)
        })
        .gamma_multiply(connection_alpha);
        ui.renderer.regions.insert(
            "Connection badge".into(),
            (
                Rect::from_center_size(center, vec2(28., 28.)),
                p.clip_rect(),
            ),
        );
        for radius in [6., 11.] {
            let points = (0..18)
                .map(|i| {
                    center
                        + vec2(
                            ((i as f32 / 17.) * 1.8 + 3.8).cos(),
                            ((i as f32 / 17.) * 1.8 + 3.8).sin(),
                        ) * radius
                })
                .collect();
            p.add(Shape::line(points, Stroke::new(1.8_f32, color)));
        }
        p.circle_filled(center + vec2(0., 3.), 1.6, color);
        p.line_segment(
            [center + vec2(-10., -10.), center + vec2(10., 9.)],
            Stroke::new(2_f32, color),
        );
    }
}
