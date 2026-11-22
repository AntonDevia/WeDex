use eframe::egui::{Pos2, Vec2};
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Area {
    #[default]
    None,
    Mini,
    Artwork,
    Zen,
    Cards,
    Scroll,
    Slider,
    Lyrics,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    None,
    Horizontal,
    Vertical,
}
#[derive(Default)]
pub struct Touch {
    pub suppress_click: bool,
    pub area: Area,
    pub axis: Axis,
    pub delta: Vec2,
    pub velocity: Vec2,
    pub scroll: Option<String>,
    last: Option<(Pos2, f64)>,
    start: Option<Pos2>,
    pub kinetic: Option<(String, f32)>,
}
impl Touch {
    pub fn begin(&mut self, p: Pos2, time: f64, area: Area, scroll: Option<String>) {
        self.suppress_click = self
            .kinetic
            .as_ref()
            .is_some_and(|(_, speed)| speed.abs() > 80.);
        self.area = area;
        self.axis = Axis::None;
        self.delta = Vec2::ZERO;
        self.velocity = Vec2::ZERO;
        self.start = Some(p);
        self.last = Some((p, time));
        self.scroll = scroll;
        self.kinetic = None;
    }
    pub fn moved(&mut self, p: Pos2, time: f64) -> f32 {
        let Some(start) = self.start else { return 0. };
        self.delta = p - start;
        if self.area == Area::Slider {
            return 0.;
        }
        if self.axis == Axis::None && self.delta.length() > 9. {
            if matches!(self.area, Area::Scroll | Area::Lyrics) {
                self.axis = Axis::Vertical;
            } else if self.delta.x.abs() > self.delta.y.abs() * 1.2 {
                self.axis = Axis::Horizontal;
            } else if self.delta.y.abs() > self.delta.x.abs() * 1.2 {
                self.axis = Axis::Vertical;
            }
        }
        let mut scroll = 0.;
        if let Some((last, at)) = self.last {
            let dt = (time - at).clamp(0.001, 0.08) as f32;
            let velocity = (p - last) / dt;
            self.velocity = self.velocity * 0.35 + velocity * 0.65;
            if self.axis == Axis::Vertical {
                scroll = p.y - last.y;
            }
        }
        self.last = Some((p, time));
        scroll
    }
    pub fn next(&self) -> Option<bool> {
        if self.axis != Axis::Horizontal
            || !matches!(self.area, Area::Mini | Area::Artwork | Area::Cards)
        {
            return None;
        }
        (self.delta.x.abs() > 52. || (self.delta.x.abs() > 22. && self.velocity.x.abs() > 650.))
            .then_some(self.delta.x < 0.)
    }
    pub fn zen(&self, height: f32) -> Option<bool> {
        if self.axis != Axis::Vertical {
            return None;
        }
        let open = self.area == Area::Mini && self.delta.y < 0.;
        let close = self.pulls_zen_down();
        ((open || close)
            && (self.delta.y.abs()
                > if open {
                    56.
                } else {
                    (height * 0.12).clamp(64., 110.)
                }
                || (self.delta.y.abs() > 28. && self.velocity.y.abs() > 700.)))
            .then_some(open)
    }
    pub fn pulls_zen_down(&self) -> bool {
        self.axis == Axis::Vertical
            && matches!(self.area, Area::Artwork | Area::Zen)
            && self.delta.y > 0.
    }
    pub fn release(&mut self) {
        if self.axis == Axis::Vertical && self.area == Area::Scroll {
            if let Some(key) = self.scroll.take().filter(|_| self.velocity.y.abs() > 80.) {
                self.kinetic = Some((key, -self.velocity.y.clamp(-4500., 4500.)));
            }
        }
        self.start = None;
        self.last = None;
    }
    pub fn coast(&mut self, dt: f32) -> Option<(String, f32)> {
        let (key, speed) = self.kinetic.as_mut()?;
        let movement = *speed * dt;
        *speed *= (-6.5 * dt).exp();
        if speed.abs() < 12. {
            self.kinetic = None;
            return None;
        }
        Some((key.clone(), movement))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directions_lock_and_sliders_do_not_become_navigation() {
        let mut t = Touch::default();
        t.begin(Pos2::ZERO, 0., Area::Artwork, None);
        t.moved(Pos2::new(-28., 2.), 0.03);
        t.moved(Pos2::new(-65., 30.), 0.06);
        assert_eq!(t.next(), Some(true));
        assert_eq!(t.zen(800.), None);
        t.begin(Pos2::ZERO, 0., Area::Slider, None);
        t.moved(Pos2::new(-150., 80.), 0.1);
        assert_eq!(t.next(), None);
        assert_eq!(t.zen(800.), None);
    }
    #[test]
    fn only_lists_coast_and_velocity_decays() {
        let mut t = Touch::default();
        t.begin(Pos2::ZERO, 0., Area::Scroll, Some("results".into()));
        t.moved(Pos2::new(0., -100.), 0.1);
        t.release();
        let a = t.coast(0.016).unwrap().1;
        let b = t.coast(0.016).unwrap().1;
        assert!(a > b && b > 0.);
        t.begin(Pos2::ZERO, 0., Area::Lyrics, None);
        t.moved(Pos2::new(0., -100.), 0.1);
        t.release();
        assert!(t.coast(0.016).is_none());
    }
}
