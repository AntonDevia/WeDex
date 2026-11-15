use super::*;
#[derive(Default)]
pub struct Cover {
    key: Option<String>,
    last: Option<Node>,
    from: Option<Node>,
    at: f64,
    pub direction: f32,
    drag: f32,
}
impl Cover {
    pub fn apply(
        &mut self,
        root: &mut Node,
        key: Option<String>,
        drag: f32,
        preview: Option<&Track>,
        time: f64,
        reduced: bool,
    ) {
        let Some(art) = root.find_mut("Zen artwork") else {
            return;
        };
        let base = art.clone();
        let width = base.width;
        if self.key != key {
            self.from = self.last.take();
            self.at = time;
            self.key = key;
            if self.direction == 0. {
                self.direction = -1.;
            }
        }
        let t = if reduced {
            1.
        } else {
            ((time - self.at) / transitions::DURATION).clamp(0., 1.) as f32
        };
        let mut children = vec![];
        if t < 1. && self.from.is_some() {
            let progress = motion::ease_out(t);
            let mut old = self.from.clone().unwrap();
            let distance = width + 12.;
            old.move_to(
                self.drag + (self.direction * distance - self.drag) * progress,
                0.,
            );
            old.name = "Outgoing artwork".into();
            old.command = None;
            let mut next = base.clone();
            next.move_to(
                (self.drag - self.direction * distance) * (1. - progress),
                0.,
            );
            next.name = "Incoming artwork".into();
            next.command = None;
            children.extend([old, next]);
        } else if drag.abs() > 0.1 {
            self.from = None;
            self.drag = drag;
            let mut current = base.clone();
            current.move_to(drag, 0.);
            current.name = "Current dragged artwork".into();
            current.command = None;
            children.push(current);
            if let Some(track) = preview {
                let mut next = base.clone();
                next.set_image(&GlassUi::source(track));
                next.move_to(
                    drag + if drag < 0. { width + 12. } else { -width - 12. },
                    0.,
                );
                next.name = "Adjacent artwork".into();
                next.command = None;
                children.push(next);
            }
        } else {
            self.from = None;
            self.drag = 0.;
        }
        self.last = Some(base);
        if !children.is_empty() {
            art.fills.clear();
            art.children = children;
            art.clips_content = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completing_a_drag_keeps_the_incoming_cover_at_the_same_position() {
        let scene = || {
            let mut root = panels::frame("root", 0., 0., 411., 800.);
            root.children
                .push(panels::frame("Zen artwork", 24., 104., 363., 363.));
            root
        };
        let tracks = demo_tracks();
        let mut cover = Cover {
            direction: -1.,
            ..Default::default()
        };
        cover.apply(&mut scene(), Some("first".into()), 0., None, 0., false);
        let mut dragged = scene();
        cover.apply(
            &mut dragged,
            Some("first".into()),
            -120.,
            Some(&tracks[1]),
            1.,
            false,
        );
        let before = dragged.find("Adjacent artwork").unwrap().x;
        let mut released = scene();
        cover.apply(&mut released, Some("second".into()), 0., None, 1.01, false);
        let after = released.find("Incoming artwork").unwrap().x;
        assert!(
            (before - after).abs() < 0.01,
            "Committing a swipe must not move the incoming artwork backwards"
        );
    }
}
