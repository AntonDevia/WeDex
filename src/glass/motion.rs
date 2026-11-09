use super::scene::Node;

/// Figma EASE_IN_AND_OUT: cubic-bezier(0.42, 0, 0.58, 1).
pub fn ease(time: f32) -> f32 {
    bezier(time, 0.42, 0.58)
}
pub fn ease_out(time: f32) -> f32 {
    bezier(time, 0., 0.58)
}
fn bezier(time: f32, x1: f32, x2: f32) -> f32 {
    let time = time.clamp(0., 1.);
    let mut low = 0.;
    let mut high = 1.;
    for _ in 0..18 {
        let t = (low + high) / 2.;
        let x = 3. * (1. - t) * (1. - t) * t * x1 + 3. * (1. - t) * t * t * x2 + t * t * t;
        if x < time {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) / 2.;
    3. * (1. - t) * t * t + t * t * t
}
pub fn ambient(time: f64) -> f32 {
    let time = (time % 14.1) as f32;
    if time < 0.05 {
        0.
    } else if time < 7.05 {
        ease((time - 0.05) / 7.)
    } else if time < 7.1 {
        1.
    } else {
        1. - ease((time - 7.1) / 7.)
    }
}
fn blend(from: &Node, to: &Node, t: f32) -> Node {
    let mix = |a: f32, b: f32| a + (b - a) * t;
    let mut out = to.clone();
    out.width = mix(from.width, to.width);
    out.height = mix(from.height, to.height);
    out.opacity = mix(from.opacity, to.opacity);
    out.corner_radius = mix(from.corner_radius, to.corner_radius);
    out.font_size = mix(from.font_size, to.font_size);
    out.stroke_weight = mix(from.stroke_weight, to.stroke_weight);
    out.natural_width = mix(from.natural_width, to.natural_width);
    let a = from
        .relative_transform
        .unwrap_or([[1., 0., from.x], [0., 1., from.y]]);
    let b = to
        .relative_transform
        .unwrap_or([[1., 0., to.x], [0., 1., to.y]]);
    let angle_a = a[1][0].atan2(a[0][0]);
    let mut delta = b[1][0].atan2(b[0][0]) - angle_a;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    let angle = angle_a + delta * t;
    let scale = mix(a[0][0].hypot(a[1][0]), b[0][0].hypot(b[1][0]));
    out.x = mix(a[0][2], b[0][2]);
    out.y = mix(a[1][2], b[1][2]);
    out.relative_transform = Some([
        [angle.cos() * scale, -angle.sin() * scale, out.x],
        [angle.sin() * scale, angle.cos() * scale, out.y],
    ]);
    for key in ["line_height", "letter_spacing"] {
        let (a, b, out) = if key == "line_height" {
            (&from.line_height, &to.line_height, &mut out.line_height)
        } else {
            (
                &from.letter_spacing,
                &to.letter_spacing,
                &mut out.letter_spacing,
            )
        };
        if let (Some(a), Some(b)) = (a["value"].as_f64(), b["value"].as_f64()) {
            out["value"] = mix(a as f32, b as f32).into();
        }
    }
    for (i, child) in out.children.iter_mut().enumerate() {
        if let Some(previous) = from.children.get(i) {
            *child = blend(previous, &to.children[i], t);
        }
    }
    out
}
#[derive(Default)]
pub struct Carousel {
    names: Vec<String>,
    from: Vec<Node>,
    shown: Vec<Node>,
    started: f64,
}
impl Carousel {
    pub fn apply(&mut self, cards: &mut Vec<Node>, time: f64, reduced: bool) {
        let names: Vec<String> = cards.iter().map(|n| n.name.clone()).collect();
        if names != self.names {
            self.from = self.shown.clone();
            self.started = time;
            self.names = names;
        }
        let progress = if reduced || self.from.is_empty() {
            1.
        } else {
            ((time - self.started) / super::transitions::DURATION).clamp(0., 1.) as f32
        };
        if progress >= 1. {
            self.from.clear();
            self.shown = cards.clone();
            return;
        }
        let t = ease_out(progress);
        let mut shown = Vec::with_capacity(cards.len() + 2);
        for target in cards.iter() {
            if let Some(from) = self.from.iter().find(|n| n.name == target.name) {
                shown.push(blend(from, target, t));
            } else {
                let mut card = target.clone();
                if progress < 1. {
                    card.opacity *= t;
                }
                shown.push(card);
            }
        }
        if progress < 1. {
            for old in &self.from {
                if !cards.iter().any(|n| n.name == old.name) {
                    let mut card = old.clone();
                    card.opacity *= 1. - t;
                    card.command = None;
                    shown.insert(0, card);
                }
            }
        }
        shown.sort_by(|a, b| {
            let scale = |n: &Node| {
                n.relative_transform
                    .map(|m| m[0][0].hypot(m[1][0]))
                    .unwrap_or(1.)
            };
            scale(a).total_cmp(&scale(b))
        });
        self.shown = shown.clone();
        *cards = shown;
    }
}
