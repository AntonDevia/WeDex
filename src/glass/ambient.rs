use eframe::egui::{pos2, vec2, Rect, Vec2};

/// Keep the original colour choreography, expressed in viewport coordinates.
/// Portrait layouts use wider blooms so their visible centres stay on the phone.
pub fn bloom(start: Rect, index: usize, phase: f32, canvas: Vec2) -> (Rect, [f32; 3]) {
    let ends = [
        (440., 70., 880., 690., [215., 47., 35.]),
        (850., 130., 650., 700., [232., 171., 37.]),
        (200., 650., 1000., 520., [147., 45., 68.]),
        (330., -120., 760., 550., [88., 35., 49.]),
    ];
    let (x, y, w, h, color) = ends[index.min(3)];
    let phase = phase.clamp(0., 1.);
    let end = Rect::from_min_size(pos2(x, y), vec2(w, h));
    let center = start.center().lerp(end.center(), phase);
    let dimensions = start.size() + (end.size() - start.size()) * phase;
    let portrait = canvas.x < canvas.y * 0.85;
    let center = pos2(
        (center.x / 1440.).clamp(0.12, 0.88) * canvas.x,
        (center.y / 960.).clamp(0.12, 0.90) * canvas.y,
    );
    let size = vec2(
        canvas.x * (dimensions.x / 1440.).max(if portrait { 1.25 } else { 0. }),
        canvas.y * (dimensions.y / 960.).max(if portrait { 0.58 } else { 0. }),
    );
    (Rect::from_center_size(center, size), color)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_bloom_stays_visible_through_the_loop_on_phones_tablets_and_desktops() {
        let starts = [
            (260., 190., 790., 650.),
            (840., 320., 580., 620.),
            (80., 590., 840., 520.),
            (700., -170., 800., 550.),
        ];
        for canvas in [
            vec2(360., 740.),
            vec2(411., 869.),
            vec2(800., 1280.),
            vec2(1440., 960.),
            vec2(2560., 1440.),
        ] {
            let viewport = Rect::from_min_size(pos2(0., 0.), canvas);
            for (index, (x, y, w, h)) in starts.into_iter().enumerate() {
                for step in 0..=20 {
                    let (rect, _) = bloom(
                        Rect::from_min_size(pos2(x, y), vec2(w, h)),
                        index,
                        step as f32 / 20.,
                        canvas,
                    );
                    assert!(viewport.contains(rect.center()));
                    assert!(rect.intersect(viewport).area() > viewport.area() * 0.05);
                    if canvas.x < canvas.y * 0.85 {
                        assert!(rect.width() >= canvas.x);
                    }
                }
            }
        }
    }
}
