use super::{panels, scene::Node, Control};
use eframe::egui::{pos2, vec2, Pos2, Rect};
use serde_json::Value;
use std::collections::HashMap;

pub const DURATION: f64 = 0.2;
#[derive(Clone, Default)]
pub(super) struct Tween {
    from: f32,
    target: f32,
    started: f64,
    initialized: bool,
}
impl Tween {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            initialized: true,
            started: 0.,
        }
    }
    pub fn value(&mut self, target: f32, time: f64, reduced: bool) -> f32 {
        if !self.initialized {
            self.from = target;
            self.target = target;
            self.initialized = true;
        }
        let shown = self.at(time);
        if self.target != target {
            self.from = shown;
            self.target = target;
            self.started = time;
        }
        if reduced {
            self.from = target;
            self.target = target;
            return target;
        }
        self.at(time)
    }
    pub fn at(&self, time: f64) -> f32 {
        let t = ((time - self.started) / DURATION).clamp(0., 1.) as f32;
        self.from + (self.target - self.from) * (t * t * (3. - 2. * t))
    }
}
#[derive(Clone)]
struct Style {
    fills: Vec<Value>,
    tint: Value,
    asset: Option<String>,
}
impl Style {
    fn of(n: &Node) -> Self {
        Self {
            fills: n.fills.clone(),
            tint: n.tint.clone(),
            asset: n.asset.clone(),
        }
    }
    fn same(&self, b: &Self) -> bool {
        self.fills == b.fills && self.tint == b.tint && self.asset == b.asset
    }
    fn mix(&self, b: &Self, t: f32) -> Self {
        fn color(a: &Value, b: &Value, t: f32) -> Value {
            let mut v = b.clone();
            for key in ["r", "g", "b", "a"] {
                if let (Some(a), Some(b)) = (a[key].as_f64(), b[key].as_f64()) {
                    v[key] = (a + (b - a) * t as f64).into();
                }
            }
            v
        }
        let mut fills = b.fills.clone();
        if self.fills.is_empty() {
            for paint in &mut fills {
                if paint["type"] == "SOLID" {
                    paint["opacity"] = (paint["opacity"].as_f64().unwrap_or(1.) * t as f64).into();
                }
            }
        }
        if fills.is_empty() && t < 1. {
            fills = self
                .fills
                .iter()
                .filter(|f| f["type"] == "SOLID")
                .cloned()
                .map(|mut f| {
                    f["opacity"] = (f["opacity"].as_f64().unwrap_or(1.) * (1. - t) as f64).into();
                    f
                })
                .collect();
        }
        for (a, b) in self
            .fills
            .iter()
            .zip(&mut fills)
            .filter(|_| !b.fills.is_empty())
        {
            if a["type"] == "SOLID" && b["type"] == "SOLID" {
                b["color"] = color(&a["color"], &b["color"], t);
                let opacity = |v: &Value| v["opacity"].as_f64().unwrap_or(1.);
                b["opacity"] = (opacity(a) + (opacity(b) - opacity(a)) * t as f64).into();
            }
        }
        Self {
            fills,
            tint: color(&self.tint, &b.tint, t),
            asset: b.asset.clone(),
        }
    }
}
struct Entry {
    from: Style,
    target: Style,
    started: f64,
    hover: Tween,
    press: Tween,
}
#[derive(Default)]
pub(super) struct Buttons {
    states: HashMap<String, Entry>,
}
impl Buttons {
    pub fn apply(
        &mut self,
        root: &mut Node,
        pointer: Option<Pos2>,
        pressed: bool,
        time: f64,
        reduced: bool,
        dark: bool,
    ) {
        self.visit(
            root,
            &format!("view/{}", root.id),
            pos2(0., 0.),
            false,
            (pointer, pressed, time, reduced, dark),
        );
        if self.states.len() > 1200 {
            self.states.clear();
        }
    }
    fn visit(
        &mut self,
        n: &mut Node,
        path: &str,
        parent: Pos2,
        in_button: bool,
        input: (Option<Pos2>, bool, f64, bool, bool),
    ) {
        if !n.visible {
            return;
        }
        let (pointer, pressed, time, reduced, dark) = input;
        let origin = parent + vec2(n.x, n.y);
        let button = (n.id == "Icon button"
            || n.command.as_ref().is_some_and(|c| {
                !matches!(
                    c,
                    Control::Noop
                        | Control::Seek
                        | Control::Volume
                        | Control::VerticalVolume
                        | Control::EqBand(_)
                        | Control::Preamp
                        | Control::Crossfade
                        | Control::LyricSeek(_)
                )
            }))
            && n.width <= 500.
            && (n.height <= 64. || n.name == "Album/sonora-start");
        let in_button = in_button || button;
        for (i, child) in n.children.iter_mut().enumerate() {
            self.visit(
                child,
                &format!("{path}/{i}/{}", child.id),
                origin,
                in_button,
                input,
            );
        }
        if !in_button {
            return;
        }
        let target = Style::of(n);
        let entry = self.states.entry(path.into()).or_insert_with(|| Entry {
            from: target.clone(),
            target: target.clone(),
            started: time - DURATION,
            hover: Tween::new(0.),
            press: Tween::new(0.),
        });
        let progress = |start: f64| {
            let t = ((time - start) / DURATION).clamp(0., 1.) as f32;
            t * t * (3. - 2. * t)
        };
        if !entry.target.same(&target) {
            entry.from = entry.from.mix(&entry.target, progress(entry.started));
            entry.target = target;
            entry.started = time;
        }
        let t = if reduced { 1. } else { progress(entry.started) };
        let style = entry.from.mix(&entry.target, t);
        n.fills = style.fills;
        n.tint = style.tint;
        if entry.from.asset != entry.target.asset && t < 1. && n.asset.is_some() {
            let mut previous = n.clone();
            previous.asset = entry.from.asset.clone();
            previous.move_to(0., 0.);
            previous.opacity = 1. - t;
            previous.command = None;
            let mut next = n.clone();
            next.move_to(0., 0.);
            next.opacity = t;
            next.command = None;
            n.asset = None;
            n.fills.clear();
            n.children = vec![previous, next];
        }
        if button {
            let size = if n.name == "Album/sonora-start" {
                let scale = n
                    .relative_transform
                    .map(|m| m[0][0].hypot(m[1][0]))
                    .unwrap_or(1.);
                vec2(n.width * scale, n.height * scale)
            } else {
                vec2(n.width, n.height)
            };
            let hovered = pointer.is_some_and(|p| Rect::from_min_size(origin, size).contains(p));
            let hover = entry
                .hover
                .value(if hovered { 1. } else { 0. }, time, reduced);
            let press = entry
                .press
                .value(if hovered && pressed { 1. } else { 0. }, time, reduced);
            if n.width <= 72. && n.height <= 72. {
                let scale = 1. + hover * 0.07 - press * 0.12;
                let mut matrix = n
                    .relative_transform
                    .unwrap_or([[1., 0., n.x], [0., 1., n.y]]);
                matrix[0][2] +=
                    (1. - scale) * (matrix[0][0] * n.width + matrix[0][1] * n.height) / 2.;
                matrix[1][2] +=
                    (1. - scale) * (matrix[1][0] * n.width + matrix[1][1] * n.height) / 2.;
                for row in &mut matrix {
                    row[0] *= scale;
                    row[1] *= scale;
                }
                n.relative_transform = Some(matrix);
            }
            if hover + press > 0.001 {
                if n.asset.is_some() {
                    let mut glyph = n.clone();
                    glyph.move_to(0., 0.);
                    glyph.command = None;
                    n.asset = None;
                    n.children = vec![glyph];
                }
                let mut shade = panels::frame("Button feedback", 0., 0., n.width, n.height);
                shade.corner_radius = n.corner_radius.max(8.);
                shade.fills = vec![panels::fill(
                    if dark {
                        [1., 1., 1.]
                    } else {
                        [0.12, 0.1, 0.15]
                    },
                    hover * 0.07 + press * 0.1,
                )];
                n.children.insert(0, shade);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zen_button_keeps_its_identity_crossfades_icons_and_animates_hover() {
        let scene = |asset: &str| {
            let mut root = panels::frame("test", 0., 0., 200., 100.);
            root.children.push(panels::icon_button(
                asset,
                10.,
                10.,
                40.,
                Control::PlayPause,
                false,
            ));
            root
        };
        let mut buttons = Buttons::default();
        let mut play = scene("play");
        buttons.apply(&mut play, None, false, 0., false, false);
        let mut pause = scene("pause");
        buttons.apply(&mut pause, None, false, 1., false, false);
        let mut midway = scene("pause");
        buttons.apply(&mut midway, None, false, 1.1, false, false);
        let glyph = &midway.children[0].children[0];
        assert!(glyph.asset.is_none());
        assert_eq!(glyph.children.len(), 2);
        assert!((glyph.children[0].opacity - 0.5).abs() < 0.001);
        assert!((glyph.children[1].opacity - 0.5).abs() < 0.001);
        let pointer = Some(pos2(30., 30.));
        let mut start = scene("pause");
        buttons.apply(&mut start, pointer, false, 2., false, false);
        let mut hover = scene("pause");
        buttons.apply(&mut hover, pointer, false, 2.1, false, false);
        assert!((hover.children[0].relative_transform.unwrap()[0][0] - 1.035).abs() < 0.001);
        let mut hovered = scene("pause");
        buttons.apply(&mut hovered, pointer, false, 2.2, false, false);
        assert!((hovered.children[0].relative_transform.unwrap()[0][0] - 1.07).abs() < 0.001);
        assert_eq!(
            hovered.children[0].children[1].asset.as_deref(),
            Some("pause")
        );
    }
    #[test]
    fn transition_finishes_in_two_hundred_milliseconds_and_reverses_continuously() {
        let mut tween = Tween::default();
        assert_eq!(tween.value(0., 0., false), 0.);
        assert_eq!(tween.value(1., 1., false), 0.);
        assert!((tween.value(1., 1.1, false) - 0.5).abs() < 0.001);
        let before = tween.at(1.1);
        assert!((tween.value(0., 1.1, false) - before).abs() < 0.001);
        assert_eq!(tween.value(0., 1.3, false), 0.);
        assert_eq!(tween.value(1., 2., true), 1.);
    }
}
